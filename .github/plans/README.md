# Quiver Development Plans

This directory contains planning documents for the Quiver audio synthesis library.

## Documents

| Document | Description |
|----------|-------------|
| [completed-features.md](completed-features.md) | All implemented improvements (P0-P3) |
| [roadmap.md](roadmap.md) | Remaining features and implementation notes |
| [examples-and-docs.md](examples-and-docs.md) | Missing examples and documentation gaps |

## Quick Status

| Priority | Description | Status |
|----------|-------------|--------|
| **P0** | Critical fixes | Complete |
| **P1** | Core modules | 9/11 done (Reverb, PitchShifter pending) |
| **P2** | Integration | Complete (external deps remain) |
| **P3** | Enhancements | 7/14 done |

## What's Next?

See [roadmap.md](roadmap.md) for:
- High complexity modules (Reverb, PitchShifter)
- External plugin format bindings
- Remaining P3 modules

## Contributing

When implementing features:

1. Check if it's listed in [roadmap.md](roadmap.md)
2. Create a feature branch
3. Implement with full port specification
4. Add unit tests (aim for >90% coverage on new code)
5. Update the relevant planning document
6. Run `make check` before PR

---

*These documents replace the previous `improvements.md` in the repository root.*
