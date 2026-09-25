# frozen_string_literal: true

require "fiddle/import"
require "json"
require "rbconfig"

module CommonMarker
  module Native
    extend Fiddle::Importer

    filename = "commonmarker_ffi.#{RbConfig::CONFIG.fetch("DLEXT")}"
    library = $LOAD_PATH
      .map { |directory| File.join(directory, "commonmarker", filename) }
      .find { |path| File.file?(path) }
    library ||= File.join(__dir__, filename)

    dlload library
    extern "void *commonmarker_call(void *, size_t)"
    extern "void commonmarker_free_string(void *)"

    module_function

    def call(operation, **arguments)
      request = JSON.generate(arguments.merge(operation: operation))
      pointer = commonmarker_call(request, request.bytesize)

      begin
        response = JSON.parse(pointer.to_s)
      ensure
        commonmarker_free_string(pointer)
      end

      raise NodeError, response.fetch("error") unless response.fetch("ok")

      response.fetch("value")
    end
  end
end
