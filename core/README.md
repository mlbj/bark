# bark-core

Core library for the `bark` reference manager ecosystem. `bark-core` contains the domain logic and data layer used by the frontends.

The crate is intentionally UI-angostic and does not depend on terminal interaction or frontent-specific behavior.

## Current modules

| Module         | Responsability                                    |
| -------------- | ------------------------------------------------- |
| `reference.rs` | Core reference model and metadata structures`     |
| `bibtex.rs`    | BibTeX parsing utilities                          | 
| `db.rs`        | Database access and persistence                   |
| `service.rs`   | High-level application services and orchestration |
| `lib.rs`       | Public crate API                                  |
