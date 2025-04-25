(use-modules ((gnu packages rust)
	      #:select (rust
			rust-analyzer))
	     ((gnu packages rust-apps)
	      #:select (rust-cargo))
	     ((gnu packages crates-io)
	      #:select (rust-clippy-0.0.302
			rust-rustup-toolchain-0.1)))

(packages->manifest (list ;rust-cargo
			  rust
			  (list rust "tools")
			  (list rust "cargo")
			  (list rust "rust-src")
			  rust-analyzer
			  rust-clippy-0.0.302
			  rust-rustup-toolchain-0.1))

