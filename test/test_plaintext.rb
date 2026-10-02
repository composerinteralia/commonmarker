# frozen_string_literal: true

require "test_helper"

class TestPlaintext < Minitest::Test
  def setup
    @markdown = <<~MD
      Hi *there*!

      1. I am a numeric list.
      2. I continue the list.
      * Suddenly, an unordered list!
      * What fun!

      Okay, _enough_.

      | a   | b   |
      | --- | --- |
      | c   | d   |
    MD
  end

  def render_doc(doc)
    CommonMarker.render_doc(doc, :DEFAULT, [:table])
  end

  def test_to_commonmark
    compare = render_doc(@markdown).to_plaintext

    assert_equal(<<~PLAINTEXT, compare)
      Hi there!

      1.  I am a numeric list.
      2.  I continue the list.

        - Suddenly, an unordered list!
        - What fun!

      Okay, enough.

      | a | b |
      | --- | --- |
      | c | d |
    PLAINTEXT
  end

  def test_collapses_spaces_in_text_but_not_code
    markdown = "Test  script  with a code block ```html script alert( hi ) /script ``` with text after"
    document = CommonMarker.render_doc(markdown, :DEFAULT, %i[strikethrough table])

    assert_equal(
      "Test script with a code block html script alert( hi ) /script  with text after\n",
      document.to_plaintext,
    )
    assert_equal(
      "Test  script  with a code block html script alert( hi ) /script  with text after\n",
      document.to_plaintext(:DEFAULT, 0),
    )
  end

  def test_softbreaks_follow_render_options
    document = CommonMarker.render_doc("*version 1*  Carbon  by  double .\nCreated by @sugar5",
      :DEFAULT, %i[strikethrough table])

    assert_equal("version 1 Carbon by double . Created by @sugar5\n", document.to_plaintext)
    assert_equal("version 1  Carbon  by  double .\nCreated by @sugar5\n", document.to_plaintext(:DEFAULT, 0))
    assert_equal("version 1  Carbon  by  double . Created by @sugar5\n", document.to_plaintext(:NOBREAKS, 0))
    assert_equal("version 1  Carbon  by  double .\nCreated by @sugar5\n", document.to_plaintext(:HARDBREAKS))
  end
end
