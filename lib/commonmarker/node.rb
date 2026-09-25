# frozen_string_literal: true

require "cgi"
require "commonmarker/native"
require "commonmarker/node/inspect"

module CommonMarker
  class NodeError < StandardError; end

  class Node
    include Enumerable
    include Inspect

    TYPES = [
      :document, :blockquote, :list, :list_item, :code_block, :html,
      :paragraph, :header, :hrule, :text, :softbreak, :linebreak, :code,
      :inline_html, :emph, :strong, :link, :image, :footnote_reference,
      :footnote_definition, :table, :table_header, :table_row, :table_cell,
      :strikethrough,
    ].freeze
    STRING_TYPES = [:code_block, :html, :text, :code, :inline_html, :footnote_reference, :footnote_definition].freeze
    LINK_TYPES = [:link, :image].freeze

    attr_reader :type, :parent

    class << self
      def markdown_to_html(text, options, extensions)
        validate_extensions(extensions)
        output = Native.call("render_markdown",
          markdown: text,
          options: options,
          extensions: extensions.map(&:to_s),
          format: "html")
        normalize_html(output, options).force_encoding(Encoding::UTF_8)
      end

      def markdown_to_xml(text, options, extensions)
        validate_extensions(extensions)
        Native.call("render_markdown",
          markdown: text,
          options: options,
          extensions: extensions.map(&:to_s),
          format: "xml").force_encoding(Encoding::UTF_8)
      end

      def parse_document(text, length, options, extensions)
        raise TypeError, "length must be an Integer" unless length.is_a?(Integer)

        validate_extensions(extensions)
        from_markdown(text.byteslice(0, length), options, extensions)
      end

      def from_markdown(markdown, options, extensions)
        node = new(:document)
        node.instance_variable_set(:@markdown, markdown)
        node.instance_variable_set(:@parse_options, options)
        node.instance_variable_set(:@parse_extensions, extensions.dup)
        node
      end

      def from_wire(wire, parent = nil)
        node = allocate
        node.send(:initialize_from_wire, wire, parent)
        node
      end

      def validate_extensions(extensions)
        raise TypeError, "extensions must be an Array" unless extensions.is_a?(Array)

        extensions.each do |extension|
          raise TypeError, "extension names should be Symbols; got a #{extension.class}" unless extension.is_a?(Symbol)
          raise ArgumentError, "extension #{extension} not found" unless CommonMarker.extensions.include?(extension.to_s)
        end

        def normalize_html(output, options)
          output = output.gsub("&#39;", "'")
          output = output.gsub(/href="([^"]*)"/) do
            %(href="#{Regexp.last_match(1).gsub("[", "%5B").gsub("]", "%5D")}")
          end
          output = output.gsub(/(<code) data-meta="([^"]*)" class="([^"]*)"/, '\1 class="\3" data-meta="\2"')

          if options & CommonMarker::Config::OPTS[:render][:SOURCEPOS] != 0
            output = output
              .gsub(/(<(?:a|code|del|em|img|strong)\b[^>]*?) data-sourcepos="[^"]*"/, '\1')
              .gsub(/<ol start="([^"]+)" data-sourcepos="([^"]+)"/, '<ol data-sourcepos="\2" start="\1"')
              .gsub(/<(th|td) align="([^"]+)" data-sourcepos="([^"]+)"/, '<\1 data-sourcepos="\3" align="\2"')
          end

          output
        end
      end
    end

    def initialize(type)
      raise TypeError, "type must be a Symbol" unless type.is_a?(Symbol)
      raise NodeError, "invalid node of type #{type}" unless TYPES.include?(type)

      @type = type
      @data = default_data(type)
      @sourcepos = [0, 0, 0, 0]
      @children = []
      @parent = nil
    end

    def walk(&block)
      return enum_for(:walk) unless block

      materialize!
      yield self
      each { |child| child.walk(&block) }
    end

    def to_html(options = :DEFAULT, extensions = [])
      opts = Config.process_options(options, :render)
      _render_html(opts, extensions).force_encoding(Encoding::UTF_8)
    end

    def to_xml(options = :DEFAULT)
      opts = Config.process_options(options, :render)
      _render_xml(opts).force_encoding(Encoding::UTF_8)
    end

    def to_commonmark(options = :DEFAULT, width = 120)
      opts = Config.process_options(options, :render)
      _render_commonmark(opts, width).force_encoding(Encoding::UTF_8)
    end

    def to_plaintext(options = :DEFAULT, width = 120)
      opts = Config.process_options(options, :render)
      _render_plaintext(opts, width).force_encoding(Encoding::UTF_8)
    end

    def each
      return enum_for(:each) unless block_given?

      materialize!
      @children.dup.each { |child| yield child }
    end

    def each_child(&block)
      warn("[DEPRECATION] `each_child` is deprecated.  Please use `each` instead.")
      each(&block)
    end

    def first_child
      materialize!
      @children.first
    end

    def last_child
      materialize!
      @children.last
    end

    def next
      sibling(1)
    end

    def previous
      sibling(-1)
    end

    def delete
      materialize!
      detach
      nil
    end

    def insert_before(node)
      materialize!
      insert_sibling(node, 0)
    end

    def insert_after(node)
      materialize!
      insert_sibling(node, 1)
    end

    def prepend_child(node)
      materialize!
      insert_child(node, 0, "prepend child")
    end

    def append_child(node)
      materialize!
      insert_child(node, @children.length, "append child")
    end

    def string_content
      materialize!
      ensure_type!(STRING_TYPES, "get string content")
      @data.fetch("literal", "").dup.force_encoding(Encoding::UTF_8)
    end

    def string_content=(value)
      raise TypeError, "string content must be a String" unless value.is_a?(String)

      materialize!
      ensure_type!(STRING_TYPES, "set string content")
      @data["literal"] = value.encode(Encoding::UTF_8)
    end

    def type_string
      return "tasklist" if type == :list_item && @data["tasklist"]

      type.to_s
    end

    def sourcepos
      materialize!
      {
        start_line: @sourcepos[0],
        start_column: @sourcepos[1],
        end_line: @sourcepos[2],
        end_column: @sourcepos[3],
      }
    end

    def url
      materialize!
      ensure_type!(LINK_TYPES, "get url")
      @data.fetch("url", "").dup
    end

    def url=(value)
      raise TypeError, "url must be a String" unless value.is_a?(String)

      materialize!
      ensure_type!(LINK_TYPES, "set url")
      @data["url"] = value
    end

    def title
      materialize!
      ensure_type!(LINK_TYPES, "get title")
      @data.fetch("title", "").dup
    end

    def title=(value)
      raise TypeError, "title must be a String" unless value.is_a?(String)

      materialize!
      ensure_type!(LINK_TYPES, "set title")
      @data["title"] = value
    end

    def header_level
      materialize!
      ensure_type!([:header], "get header_level")
      @data.fetch("header_level")
    end

    def header_level=(value)
      raise TypeError, "header level must be an Integer" unless value.is_a?(Integer)
      raise NodeError, "could not set header_level" unless (1..6).cover?(value)

      materialize!
      ensure_type!([:header], "set header_level")
      @data["header_level"] = value
    end

    def list_type
      materialize!
      ensure_type!([:list], "get list_type")
      @data.fetch("list_type").to_sym
    end

    def list_type=(value)
      raise TypeError, "list type must be a Symbol" unless value.is_a?(Symbol)
      raise NodeError, "invalid list_type" unless [:bullet_list, :ordered_list].include?(value)

      materialize!
      ensure_type!([:list], "set list_type")
      @data["list_type"] = value.to_s
    end

    def list_start
      materialize!
      ensure_type!([:list], "get list_start")
      raise NodeError, "can't get list_start for non-ordered list" unless list_type == :ordered_list

      @data.fetch("list_start")
    end

    def list_start=(value)
      raise TypeError, "list start must be an Integer" unless value.is_a?(Integer)

      materialize!
      ensure_type!([:list], "set list_start")
      @data["list_start"] = value
    end

    def list_tight
      materialize!
      ensure_type!([:list], "get list_tight")
      @data.fetch("list_tight")
    end

    def list_tight=(value)
      materialize!
      ensure_type!([:list], "set list_tight")
      @data["list_tight"] = !!value
    end

    def fence_info
      materialize!
      ensure_type!([:code_block], "get fence_info")
      @data.fetch("fence_info", "").dup
    end

    def fence_info=(value)
      raise TypeError, "fence info must be a String" unless value.is_a?(String)

      materialize!
      ensure_type!([:code_block], "set fence_info")
      @data["fence_info"] = value
    end

    def table_alignments
      materialize!
      ensure_type!([:table], "get table alignments")
      @data.fetch("table_alignments").map { |alignment| alignment == "none" ? nil : alignment.to_sym }
    end

    def tasklist_item_checked?
      materialize!
      !!@data["checked"]
    end

    def tasklist_item_checked=(value)
      materialize!
      raise NodeError, "could not set tasklist_item_checked" unless type == :list_item && @data["tasklist"]

      @data["checked"] = !!value
    end

    def tasklist_state
      tasklist_item_checked? ? "checked" : "unchecked"
    end

    def html_escape_html(value)
      raise TypeError, "text must be a String" unless value.is_a?(String)

      value.gsub("&", "&amp;").gsub("<", "&lt;").gsub(">", "&gt;").gsub('"', "&quot;")
    end

    def html_escape_href(value)
      raise TypeError, "text must be a String" unless value.is_a?(String)

      CGI.escapeHTML(value.gsub(/[^A-Za-z0-9\-._~:\/?#@!$&'()*+,;=%]/) { |character| character.bytes.map { |byte| format("%%%02X", byte) }.join })
    end

    def _render_html(options, extensions)
      render_native("html", options, extensions)
    end

    def _render_xml(options)
      render_native("xml", options, [])
    end

    def _render_commonmark(options, width = 120)
      render_native("commonmark", options, [], width)
    end

    def _render_plaintext(options, width = 120)
      render_native("plaintext", options, [], width)
    end

    def to_wire
      materialize!
      {
        kind: type.to_s,
        data: @data,
        sourcepos: @sourcepos,
        children: @children.map(&:to_wire),
      }
    end

    private

    def initialize_from_wire(wire, parent)
      @type = wire.fetch("kind").to_sym
      raise NodeError, "unsupported node type #{@type}" unless TYPES.include?(@type)

      @data = wire.fetch("data", {})
      @sourcepos = wire.fetch("sourcepos", [0, 0, 0, 0])
      @parent = parent
      @children = wire.fetch("children", []).map { |child| self.class.from_wire(child, self) }
      @markdown = nil
      @parse_options = nil
      @parse_extensions = nil
    end

    def default_data(type)
      case type
      when :list
        { "list_type" => "bullet_list", "list_start" => 0, "list_tight" => false, "delimiter" => "period", "bullet_char" => 45 }
      when :code_block
        { "literal" => "", "fence_info" => "", "fenced" => false, "fence_char" => 96, "fence_length" => 3 }
      when :header
        { "header_level" => 1, "setext" => false }
      when :link, :image
        { "url" => "", "title" => "" }
      when *STRING_TYPES
        { "literal" => "" }
      else
        {}
      end
    end

    def sibling(offset)
      return unless parent

      index = parent.send(:child_index, self)
      parent.send(:child_at, index + offset)
    end

    def child_index(node)
      @children.index(node)
    end

    def child_at(index)
      return if index.negative?

      @children[index]
    end

    def detach
      return unless parent

      parent.send(:remove_child, self)
      @parent = nil
    end

    def remove_child(node)
      @children.delete(node)
    end

    def insert_sibling(node, offset)
      raise NodeError, "could not insert #{offset.zero? ? "before" : "after"}" unless parent
      raise NodeError, "could not insert #{offset.zero? ? "before" : "after"}" if node.equal?(self)

      index = parent.send(:child_index, self)
      parent.send(:insert_child, node, index + offset, offset.zero? ? "insert before" : "insert after")
    end

    def insert_child(node, index, operation)
      raise TypeError, "child must be a CommonMarker::Node" unless node.is_a?(Node)
      raise NodeError, "could not #{operation}" if node.type == :document || node.equal?(self) || ancestor_of?(node)

      node.send(:detach)
      @children.insert(index, node)
      node.instance_variable_set(:@parent, self)
      true
    end

    def ancestor_of?(node)
      current = self
      while current
        return true if current.equal?(node)

        current = current.parent
      end
      false
    end

    def ensure_type!(types, operation)
      raise NodeError, "could not #{operation}" unless types.include?(type)
    end

    def render_native(format, options, extensions, width = nil)
      self.class.validate_extensions(extensions)
      if @markdown
        active_extensions = (@parse_extensions + extensions).uniq
        parse_bits = @parse_options & (
          CommonMarker::Config::OPTS[:parse][:SMART] |
          CommonMarker::Config::OPTS[:parse][:LIBERAL_HTML_TAG] |
          CommonMarker::Config::OPTS[:parse][:FOOTNOTES] |
          CommonMarker::Config::OPTS[:parse][:STRIKETHROUGH_DOUBLE_TILDE]
        )
        output = Native.call("render_markdown",
          markdown: @markdown,
          options: parse_bits | options,
          extensions: active_extensions.map(&:to_s),
          format: format,
          width: width)
        return normalize_output(output, format, options)
      end

      output = Native.call("render_ast",
        node: to_wire,
        options: options,
        extensions: extensions.map(&:to_s),
        format: format,
        width: width)
      normalize_output(output, format, options, @markdown)
    end

    def materialize!
      return unless @markdown

      wire = Native.call("parse",
        markdown: @markdown,
        options: @parse_options,
        extensions: @parse_extensions.map(&:to_s))
      initialize_from_wire(wire, @parent)
    end

    def normalize_output(output, format, options, markdown = nil)
      case format
      when "html"
        self.class.normalize_html(output, options)
      when "xml"
        normalize_xml(output)
      else
        output
      end
    end

    def normalize_xml(output)
      output.gsub(/(<table\b.*?)(<table_row\b.*?<\/table_row>)(.*?<\/table>)/m) do
        table_start = Regexp.last_match(1)
        first_row = Regexp.last_match(2)
        table_end = Regexp.last_match(3)
        first_row = first_row
          .sub("<table_row", "<table_header")
          .sub("</table_row>", "</table_header>")
        "#{table_start}#{first_row}#{table_end}"
      end
    end
  end
end
