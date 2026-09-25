# frozen_string_literal: true

require "commonmarker/node/ast"
require "commonmarker/node/inspect"

module Commonmarker
  class Node
    include Enumerable
    include Inspect

    STRING_CONTENT_TYPES = [:text, :code, :code_block].freeze
    LITERAL_TYPES = [:text, :code, :code_block, :html_block, :html_inline, :raw, :math, :frontmatter].freeze
    LINK_TYPES = [:link, :image].freeze
    ALERT_TYPES = [:note, :tip, :important, :warning, :caution].freeze
    SIMPLE_TYPES = [
      :document, :block_quote, :description_list, :description_term, :description_details,
      :paragraph, :thematic_break, :table_cell, :softbreak, :linebreak, :emph, :strong,
      :strikethrough, :highlight, :insert, :superscript, :escaped, :underline, :subscript,
      :spoiler, :subtext,
    ].freeze

    attr_reader :type, :parent

    def self.from_wire(wire)
      node = allocate
      node.send(:initialize_from_wire, wire)
      node
    end

    def initialize(type, **arguments)
      @type = type.to_sym
      @data = build_data(@type, arguments)
      @source_position = [0, 0, 0, 0]
      @children = []
      @parent = nil
    end

    def respond_to?(name, include_all = false)
      return false if node_supports?(name.to_sym) == false

      super
    end

    def walk(&block)
      return enum_for(:walk) unless block

      yield self
      each { |child| child.walk(&block) }
    end

    def each
      return enum_for(:each) unless block_given?

      child = first_child
      while child
        next_child = child.next_sibling
        yield child
        child = next_child
      end
    end

    def first_child
      @children.first
    end

    def last_child
      @children.last
    end

    def previous_sibling
      sibling(-1)
    end

    def next_sibling
      sibling(1)
    end

    def prepend_child(node)
      attach_child(node, 0)
    end

    def append_child(node)
      attach_child(node, @children.length)
    end

    def insert_before(node)
      insert_sibling(node, 0)
    end

    def insert_after(node)
      insert_sibling(node, 1)
    end

    def replace(node)
      return true unless parent

      index = parent.send(:children).index(self)
      node.send(:detach)
      parent.send(:children)[index] = node
      node.send(:parent=, parent)
      @parent = nil
      true
    end

    def delete
      detach
      self
    end

    def source_position
      {
        start_line: @source_position[0],
        start_column: @source_position[1],
        end_line: @source_position[2],
        end_column: @source_position[3],
      }
    end

    def string_content
      ensure_type!(STRING_CONTENT_TYPES, "node does not have string content")
      @type == :text ? @data["content"] : @data["literal"]
    end

    def string_content=(content)
      ensure_type!(STRING_CONTENT_TYPES, "node does not have string content")
      @data[@type == :text ? "content" : "literal"] = String(content)
      true
    end

    def literal
      ensure_type!(LITERAL_TYPES, "node does not have a literal")
      @data[literal_key]
    end

    def literal=(literal)
      ensure_type!(LITERAL_TYPES, "node does not have a literal")
      @data[literal_key] = String(literal)
      true
    end

    def url
      ensure_type!(LINK_TYPES, "node is not an image or link node")
      @data["url"]
    end

    def url=(url)
      ensure_type!(LINK_TYPES, "node is not an image or link node")
      @data["url"] = String(url)
      true
    end

    def title
      ensure_type!(LINK_TYPES, "node is not an image or link node")
      @data["title"]
    end

    def title=(title)
      ensure_type!(LINK_TYPES, "node is not an image or link node")
      @data["title"] = String(title)
      true
    end

    def header_level
      ensure_type!([:heading], "node is not a heading node")
      @data["level"]
    end

    def header_level=(level)
      ensure_type!([:heading], "node is not a heading node")
      @data["level"] = Integer(level)
      true
    end

    def list_type
      ensure_type!([:list], "node is not a list node")
      @data["type"].to_sym
    end

    def list_type=(type)
      ensure_type!([:list], "node is not a list node")
      @data["type"] = type.to_s if [:bullet, :ordered].include?(type.to_sym)
      true
    end

    def list_start
      ensure_type!([:list], "node is not a list node")
      @data["start"]
    end

    def list_start=(start)
      ensure_type!([:list], "node is not a list node")
      @data["start"] = Integer(start)
      true
    end

    def list_tight
      ensure_type!([:list], "node is not a list node")
      @data["tight"]
    end

    def list_tight=(tight)
      ensure_type!([:list], "node is not a list node")
      @data["tight"] = !!tight
      true
    end

    def fenced?
      ensure_type!([:code_block], "node is not a code block node")
      @data["fenced"]
    end

    def fenced=(fenced)
      ensure_type!([:code_block], "node is not a code block node")
      @data["fenced"] = !!fenced
      true
    end

    def fence_info
      ensure_type!([:code_block], "node is not a code block node")
      @data["info"]
    end

    def fence_info=(info)
      ensure_type!([:code_block], "node is not a code block node")
      @data["info"] = String(info)
      true
    end

    def alert_type
      ensure_type!([:alert], "node is not an alert node")
      @data["type"].to_sym
    end

    def alert_type=(type)
      ensure_type!([:alert], "node is not an alert node")
      @data["type"] = type.to_s if ALERT_TYPES.include?(type.to_sym)
      true
    end

    def node_supports?(name)
      property = name.to_s.delete_suffix("=").delete_suffix("?").to_sym
      case property
      when :string_content then STRING_CONTENT_TYPES.include?(@type)
      when :literal then LITERAL_TYPES.include?(@type)
      when :url, :title then LINK_TYPES.include?(@type)
      when :header_level then @type == :heading
      when :list_type, :list_start, :list_tight then @type == :list
      when :fenced, :fence_info then @type == :code_block
      when :alert_type then @type == :alert
      end
    end

    def to_html(options: Commonmarker::Config::OPTIONS, plugins: Commonmarker::Config::PLUGINS)
      render(:html, options, plugins)
    end

    def to_commonmark(options: Commonmarker::Config::OPTIONS, plugins: Commonmarker::Config::PLUGINS)
      render(:commonmark, options, plugins)
    end

    def to_wire
      {
        "kind" => @type.to_s,
        "data" => @data,
        "source_position" => @source_position,
        "children" => @children.map(&:to_wire),
      }
    end

    protected

    attr_reader :children

    def parent=(node)
      @parent = node
    end

    private

    def initialize_from_wire(wire)
      @type = wire.fetch("kind").to_sym
      @data = wire.fetch("data", {})
      @source_position = wire.fetch("source_position", [0, 0, 0, 0])
      @parent = nil
      @children = wire.fetch("children", []).map do |child_wire|
        child = self.class.from_wire(child_wire)
        child.send(:parent=, self)
        child
      end
    end

    def build_data(type, arguments)
      args = arguments.transform_keys(&:to_sym)
      return {} if SIMPLE_TYPES.include?(type)

      case type
      when :frontmatter
        { "literal" => args.fetch(:literal, "") }
      when :footnote_definition
        { "name" => String(args.fetch(:name)), "total_references" => Integer(args.fetch(:total_references, 1)) }
      when :list, :item
        list_type = args.fetch(:type).to_sym
        raise ArgumentError, "list type must be `bullet` or `ordered`" unless [:bullet, :ordered].include?(list_type)

        {
          "type" => list_type.to_s,
          "marker_offset" => Integer(args.fetch(:marker_offset, 0)),
          "padding" => Integer(args.fetch(:padding, 0)),
          "start" => Integer(args.fetch(:start, 0)),
          "delimiter" => args.fetch(:delimiter, ".").to_s,
          "bullet_char" => Integer(args.fetch(:bullet_char, 0)),
          "tight" => !!args.fetch(:tight, false),
          "task_list" => !!args.fetch(:task_list, false),
        }
      when :description_item
        {
          "marker_offset" => Integer(args.fetch(:marker_offset, 0)),
          "padding" => Integer(args.fetch(:padding, 0)),
          "tight" => !!args.fetch(:tight, false),
        }
      when :code_block
        {
          "fenced" => !!args.fetch(:fenced),
          "fence_char" => Integer(args.fetch(:fence_char, "`".ord)),
          "fence_length" => Integer(args.fetch(:fence_length, 0)),
          "fence_offset" => Integer(args.fetch(:fence_offset, 0)),
          "info" => String(args.fetch(:info, "")),
          "literal" => String(args.fetch(:literal, "")),
          "closed" => !!args.fetch(:closed, true),
        }
      when :html_block
        { "block_type" => Integer(args.fetch(:block_type, 0)), "literal" => String(args.fetch(:literal, "")) }
      when :heading
        { "level" => Integer(args.fetch(:level)), "setext" => !!args.fetch(:setext, false), "closed" => !!args.fetch(:closed, false) }
      when :table
        {
          "alignments" => args.fetch(:alignments).map(&:to_s),
          "num_columns" => Integer(args.fetch(:num_columns)),
          "num_rows" => Integer(args.fetch(:num_rows)),
          "num_nonempty_cells" => Integer(args.fetch(:num_nonempty_cells)),
        }
      when :table_row
        { "header" => !!args.fetch(:header) }
      when :text
        { "content" => String(args.fetch(:content, "")) }
      when :taskitem
        { "mark" => args[:mark]&.to_s }
      when :code
        { "num_backticks" => Integer(args.fetch(:num_backticks, 1)), "literal" => String(args.fetch(:literal, "")) }
      when :html_inline, :raw
        { "content" => String(args.fetch(:content, "")) }
      when :link, :image
        { "url" => String(args.fetch(:url)), "title" => String(args.fetch(:title, "")) }
      when :footnote_reference
        {
          "name" => String(args.fetch(:name)),
          "texts" => args.fetch(:texts, []),
          "ref_num" => Integer(args.fetch(:ref_num, 0)),
          "ix" => Integer(args.fetch(:ix, 0)),
        }
      when :shortcode
        { "code" => String(args.fetch(:code)), "emoji" => String(args.fetch(:emoji, "")) }
      when :math
        {
          "dollar_math" => !!args.fetch(:dollar_math),
          "display_math" => !!args.fetch(:display_math),
          "literal" => String(args.fetch(:literal)),
        }
      when :multiline_block_quote
        { "fence_length" => Integer(args.fetch(:fence_length)), "fence_offset" => Integer(args.fetch(:fence_offset)) }
      when :wikilink
        { "url" => String(args.fetch(:url)) }
      when :escaped_tag
        { "tag" => String(args.fetch(:tag)) }
      when :alert
        alert_type = args.fetch(:type).to_sym
        raise ArgumentError, "invalid alert type" unless ALERT_TYPES.include?(alert_type)

        {
          "type" => alert_type.to_s,
          "title" => args[:title]&.to_s,
          "multiline" => !!args.fetch(:multiline, false),
          "fence_length" => Integer(args.fetch(:fence_length, 0)),
          "fence_offset" => Integer(args.fetch(:fence_offset, 0)),
        }
      when :block_directive
        {
          "fence_length" => Integer(args.fetch(:fence_length, 0)),
          "fence_offset" => Integer(args.fetch(:fence_offset, 0)),
          "info" => String(args.fetch(:info, "")),
        }
      else
        raise ArgumentError, "unknown node type #{type}"
      end
    rescue KeyError => error
      raise ArgumentError, error.message
    end

    def render(format, options, plugins)
      raise TypeError, "options must be a Hash; got a #{options.class}!" unless options.is_a?(Hash)

      opts = Config.process_options(options)
      processed_plugins = Config.process_plugins(plugins)
      Native.call(:render_ast, node: to_wire, format: format, options: opts, plugins: processed_plugins).force_encoding("utf-8")
    end

    def literal_key
      [:text, :html_inline, :raw].include?(@type) ? "content" : "literal"
    end

    def ensure_type!(types, message)
      raise TypeError, message unless types.include?(@type)
    end

    def sibling(offset)
      return unless parent

      index = parent.send(:children).index(self)
      parent.send(:children)[index + offset] if index && index + offset >= 0
    end

    def attach_child(node, index)
      ensure_node!(node)
      node.send(:detach)
      @children.insert(index, node)
      node.send(:parent=, self)
      true
    end

    def insert_sibling(node, offset)
      ensure_node!(node)
      return true unless parent

      target_parent = parent
      index = target_parent.send(:children).index(self)
      node.send(:detach)
      target_parent.send(:children).insert(index + offset, node)
      node.send(:parent=, target_parent)
      true
    end

    def detach
      @parent&.send(:children)&.delete(self)
      @parent = nil
    end

    def ensure_node!(node)
      raise TypeError, "expected a Commonmarker::Node" unless node.is_a?(self.class)
    end
  end
end
