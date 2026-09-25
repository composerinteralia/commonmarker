# frozen_string_literal: true

require "fiddle"
require "json"
require "rbconfig"

module Commonmarker
  module Native
    module_function

    def call(operation, **arguments)
      request = JSON.generate(arguments.merge(operation: operation.to_s))
      pointer = call_function.call(request, request.bytesize)
      raise RuntimeError, "commonmarker native call returned a null pointer" if pointer.to_i.zero?

      response = JSON.parse(Fiddle::Pointer.new(pointer).to_s)
      return response.fetch("value") if response.fetch("ok")

      error = response.fetch("error")
      exception = {
        "argument" => ArgumentError,
        "type" => TypeError,
        "runtime" => RuntimeError,
      }.fetch(error.fetch("kind"), RuntimeError)
      raise exception, error.fetch("message")
    ensure
      free_function.call(pointer) if pointer && !pointer.to_i.zero?
    end

    def call_function
      @call_function ||= Fiddle::Function.new(
        library["commonmarker_call"],
        [Fiddle::TYPE_VOIDP, Fiddle::TYPE_SIZE_T],
        Fiddle::TYPE_VOIDP,
      )
    end

    def free_function
      @free_function ||= Fiddle::Function.new(
        library["commonmarker_free"],
        [Fiddle::TYPE_VOIDP],
        Fiddle::TYPE_VOID,
      )
    end

    def library
      @library ||= Fiddle.dlopen(library_path)
    end

    def library_path
      extension = RbConfig::CONFIG.fetch("DLEXT")
      ruby_version = RUBY_VERSION[/\d+\.\d+/]
      relative_paths = [
        File.join(ruby_version, "commonmarker.#{extension}"),
        "commonmarker.#{extension}",
      ]
      candidates = relative_paths.map { |path| File.expand_path(path, __dir__) }
      $LOAD_PATH.each do |load_path|
        relative_paths.each do |path|
          candidates << File.join(load_path, "commonmarker", path)
        end
      end

      candidates.find { |path| File.file?(path) } ||
        raise(LoadError, "cannot find the commonmarker native library")
    end
  end
end
