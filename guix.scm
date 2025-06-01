(use-modules (guix gexp)
	     (guix utils)
	     (guix packages)
	     (guix git)
	     (guix git-download)
	     (guix build-system cargo)
	     (gnu packages crates-io)
	     ((guix licenses) #:prefix license:))

(define vcs-file?
  ;; Return true if the given file is under version control.
  (or (git-predicate (current-source-directory))
      (const #t))) ; not in a Git checkout

(package
  (name "bos-cli")
  (version "0")
  ;(source (git-checkout (url (dirname (current-filename)))))
  (source (local-file "." "bos-cli-checkout"
		      #:recursive? #t
		      #:select? vcs-file?))
  (build-system cargo-build-system)
  (arguments
    `(#:cargo-inputs (("rust-anyhow" ,rust-anyhow-1)
		      ("rust-clap" ,rust-clap-4)
		      ("rust-serde" ,rust-serde-1)
		      ("rust-toml" ,rust-toml-0.8))))
  (home-page
    "https://github.com/Aloso/to-html/tree/master/crates/ansi-to-html")
  (synopsis "ANSI escape codes to HTML converter")
  (description "This package provides an ANSI escape codes to HTML converter.")
  (license license:expat))

