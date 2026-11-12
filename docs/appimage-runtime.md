# AppImage runtime

Build with `python3 scripts/release.py`; publish with `python3 scripts/publish.py`.

The release script contains all runtime packaging and checks. It bundles glibc, its matching loader, the export renderer, libraries, and fallback fonts. Older hosts use the private runtime; compatible newer hosts use their own glibc. Graphics drivers remain host supplied. Use DARAH_RUNTIME_DEBUG=1 for runtime diagnostics.
