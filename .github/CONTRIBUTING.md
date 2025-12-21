# Contributing to Quiver

Thank you for your interest in contributing to Quiver! This document provides guidelines and information about contributing to this project.

## Table of Contents

- [Code of Conduct](#code-of-conduct)
- [Getting Started](#getting-started)
- [Development Workflow](#development-workflow)
- [Pull Request Process](#pull-request-process)
- [Coding Standards](#coding-standards)
- [Testing Guidelines](#testing-guidelines)
- [Commit Messages](#commit-messages)

## Code of Conduct

This project follows the [Rust Code of Conduct](https://www.rust-lang.org/policies/code-of-conduct). Please be respectful and constructive in all interactions.

## Getting Started

1. **Fork the repository** and clone your fork locally
2. **Install Rust** (1.70 or later) via [rustup](https://rustup.rs/)
3. **Install development tools**:
   ```bash
   make setup
   ```
4. **Run the test suite** to verify your setup:
   ```bash
   make test
   ```

## Development Workflow

### Branch Naming

Use descriptive branch names:
- `feature/add-delay-module` - New features
- `fix/vco-drift-calculation` - Bug fixes
- `docs/update-filter-tutorial` - Documentation changes
- `refactor/optimize-graph-traversal` - Refactoring

### Making Changes

1. Create a new branch from `main`
2. Make your changes
3. Run checks locally before pushing:
   ```bash
   make check
   ```
4. Push to your fork and open a Pull Request

### Running Checks

```bash
# Run all checks (format, lint, test)
make check

# Individual commands
make fmt      # Format code
make lint     # Run clippy
make test     # Run tests
make coverage # Run tests with coverage
make bench    # Run benchmarks
```

## Pull Request Process

1. **Fill out the PR template** completely
2. **Link related issues** using keywords (`Fixes #123`, `Closes #456`)
3. **Ensure CI passes** - all checks must be green
4. **Request review** from maintainers
5. **Address feedback** promptly and push updates
6. **Squash commits** if requested before merge

### PR Requirements

- [ ] Tests pass (`cargo test --all-features`)
- [ ] Code is formatted (`cargo fmt`)
- [ ] Clippy passes (`cargo clippy -- -D warnings`)
- [ ] Documentation updated if needed
- [ ] Changelog entry added for notable changes

## Coding Standards

### Rust Style

- Follow [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- Use `rustfmt` for formatting (configuration in `rustfmt.toml` if present)
- Address all Clippy warnings

### Documentation

- Document all public items with doc comments
- Include examples in documentation where helpful
- Use `///` for item documentation, `//!` for module documentation

### Error Handling

- Use `Result` for operations that can fail
- Provide meaningful error messages
- Avoid `unwrap()` in library code (ok in tests/examples)

## Testing Guidelines

### Test Coverage

We maintain a minimum of 80% code coverage. New code should include tests.

### Test Categories

1. **Unit Tests**: Test individual functions and modules
   ```rust
   #[cfg(test)]
   mod tests {
       use super::*;

       #[test]
       fn test_vco_frequency() {
           let vco = Vco::new();
           // ...
       }
   }
   ```

2. **Integration Tests**: Test module interactions (in `tests/`)

3. **Doc Tests**: Ensure documentation examples compile and run

### Running Tests

```bash
# All tests
cargo test --all-features

# Specific test
cargo test test_vco_frequency

# With coverage
cargo tarpaulin --all-features
```

## Commit Messages

We follow [Conventional Commits](https://www.conventionalcommits.org/) format:

```
<type>(<scope>): <description>

[optional body]

[optional footer]
```

### Types

- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation changes
- `style`: Formatting, missing semicolons, etc.
- `refactor`: Code change that neither fixes a bug nor adds a feature
- `perf`: Performance improvement
- `test`: Adding or updating tests
- `chore`: Build process, auxiliary tool changes

### Examples

```
feat(modules): add delay line module with feedback

Implements a basic delay line with:
- Configurable delay time (0-5 seconds)
- Feedback control with saturation
- Wet/dry mix

Closes #42
```

```
fix(svf): correct resonance calculation at high frequencies

The resonance was becoming unstable above 10kHz due to
numerical precision issues. Added coefficient limiting.

Fixes #78
```

## Areas for Contribution

### High Priority

- **DSP Algorithms**: More accurate filter models, oscillator antialiasing
- **Testing**: Audio comparison tests, performance benchmarks
- **Documentation**: Tutorials, examples, API docs

### Medium Priority

- **Modules**: Classic hardware module behaviors
- **Optimization**: SIMD implementations, cache efficiency
- **Tooling**: Better debugging/visualization tools

### Good First Issues

Look for issues labeled [`good first issue`](https://github.com/alexnodeland/quiver/labels/good%20first%20issue) for beginner-friendly tasks.

## Questions?

- Open a [Discussion](https://github.com/alexnodeland/quiver/discussions) for general questions
- Open an [Issue](https://github.com/alexnodeland/quiver/issues) for bugs or feature requests

Thank you for contributing!
