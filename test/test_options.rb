# frozen_string_literal: true

require "test_helper"

class TestExtensions < Minitest::Test
  def test_full_info_string
    md = <<~MD
      ```ruby
      module Foo
      ```
    MD

    CommonMarker.render_html(md, :FULL_INFO_STRING).tap do |out|
      assert_includes(out, '<pre><code class="language-ruby">')
    end

    md = <<~MD
      ```ruby my info string
      module Foo
      ```
    MD

    CommonMarker.render_html(md, :FULL_INFO_STRING).tap do |out|
      assert_includes(out, '<pre><code class="language-ruby" data-meta="my info string">')
    end

    md = <<~MD
      ```ruby my \x00 string
      module Foo
      ```
    MD

    CommonMarker.render_html(md, :FULL_INFO_STRING).tap do |out|
      assert_includes(out, %(<pre><code class="language-ruby" data-meta="my � string">))
    end
  end

  def test_liberal_html_tag
    word = "galatasaray osmanlispor maci canli izle"
    tagged_word = word.each_char.map { |character| character == " " ? character : "<K!%K>#{character}</K!%K>" }.join
    markdown = %(<placeholder class="border">#<a href="https://example.com">#{tagged_word}</a></placeholder>)
    extensions = [:table, :strikethrough, :tagfilter, :autolink]

    document = CommonMarker.render_doc(markdown, :LIBERAL_HTML_TAG, extensions)

    assert_equal(
      "<p><placeholder class=\"border\">#<a href=\"https://example.com\">#{tagged_word}</a></placeholder></p>\n",
      document.to_html([:UNSAFE, :GITHUB_PRE_LANG, :HARDBREAKS], extensions),
    )

    escaped_document = CommonMarker.render_doc(markdown, :DEFAULT, extensions)

    assert_includes(escaped_document.to_html(:UNSAFE, extensions), "&lt;K!%K&gt;g&lt;/K!%K&gt;")

    escaped_tags = CommonMarker.render_doc('\<K!%K>x</K!%K> &lt;K!%K>x&lt;/K!%K&gt;', :LIBERAL_HTML_TAG)

    assert_equal(
      "<p>&lt;K!%K&gt;x&lt;/K!%K&gt; &lt;K!%K&gt;x&lt;/K!%K&gt;</p>\n",
      escaped_tags.to_html(:UNSAFE),
    )
  end

  def test_preserves_legacy_numeric_noncharacter_bytes
    extensions = [:table, :strikethrough, :tagfilter, :autolink]
    render_options = [:UNSAFE, :GITHUB_PRE_LANG, :HARDBREAKS]
    markdown = "&#xFFFE; \uFFFE &#65535; \uFFFF `&#xFFFE; &#xFFFF;`"

    output = CommonMarker.render_doc(markdown, :LIBERAL_HTML_TAG, extensions)
      .to_html(render_options, extensions)

    expected = "<p>\xFE \uFFFE \xFF \uFFFF <code>&amp;#xFFFE; &amp;#xFFFF;</code></p>\n"

    assert_equal(expected.b, output.b)
    refute_predicate(output, :valid_encoding?)
  end

  def test_liberal_html_tags_with_nbsp_attribute_whitespace
    markdown = " \n<b></b>\n \n<c></c>\n<a   href='http://hoz.blogspot.com'> <img  src='http://example.com/whatever.jpg'  ></a>"
    extensions = [:table, :strikethrough, :tagfilter, :autolink]

    output = CommonMarker.render_doc(markdown, :LIBERAL_HTML_TAG, extensions)
      .to_html([:UNSAFE, :GITHUB_PRE_LANG, :HARDBREAKS], extensions)

    assert_equal(
      "<p> <br />\n<b></b><br />\n <br />\n<c></c><br />\n<a   href='http://hoz.blogspot.com'> <img  src='http://example.com/whatever.jpg'  ></a></p>\n",
      output,
    )
  end
end
