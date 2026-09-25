# frozen_string_literal: true

require "rbconfig"

extension = RbConfig::CONFIG.fetch("DLEXT")
cargo_library = case RbConfig::CONFIG.fetch("host_os")
when /darwin/
  "libcommonmarker_ffi.dylib"
when /mswin|mingw/
  "commonmarker_ffi.dll"
else
  "libcommonmarker_ffi.so"
end

File.write("Makefile", <<~MAKE)
  RUBY = #{RbConfig.ruby}
  CARGO ?= cargo
  CARGO_MANIFEST = #{File.expand_path("Cargo.toml", __dir__)}
  CARGO_LOCKFILE = #{File.expand_path("Cargo.lock", __dir__)}
  RUST_SOURCES = #{Dir.glob(File.expand_path("src/**/*.rs", __dir__)).join(" ")}
  CARGO_TARGET_DIR = $(CURDIR)/cargo-target
  CARGO_LIBRARY = $(CARGO_TARGET_DIR)/release/#{cargo_library}
  TARGET = commonmarker_ffi.#{extension}

  all: $(TARGET)

  $(TARGET): $(CARGO_MANIFEST) $(CARGO_LOCKFILE) $(RUST_SOURCES)
  \t$(CARGO) build --release --manifest-path "$(CARGO_MANIFEST)" --target-dir "$(CARGO_TARGET_DIR)"
  \tcp "$(CARGO_LIBRARY)" "$(TARGET)"

  install: all
  \t$(RUBY) -rfileutils -e 'FileUtils.mkdir_p(ARGV[0]); FileUtils.cp(ARGV[1], ARGV[0])' "$(sitearchdir)/commonmarker" "$(TARGET)"

  clean:
  \trm -rf "$(CARGO_TARGET_DIR)" "$(TARGET)"
MAKE
