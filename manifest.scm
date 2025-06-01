(use-modules ((gnu packages rust)
	      #:select (rust
			rust-analyzer))
	     ((btv rust)
	      #:select (rust-next))
	     ((gnu packages terminals)
	      #:select (alacritty))
	     ((gnu packages rust-apps)
	      #:select (rust-cargo))
	     (gnu packages crates-io))
	      ;#:select (rust-clippy-0.0.302
	;		rust-proc-macro2-1)))

(concatenate-manifests
  (list ;(package->development-manifest (load "guix.scm"))
	(packages->manifest (list rust-next
				  (list rust-next "tools")
				  (list rust-next "cargo")
				  (list rust-next "rust-src")
				  rust-analyzer
				  rust-clippy-0.0.302
				  rust-serde-1
				  rust-clap-4
				  rust-toml-0.8
				  rust-anyhow-1))))

