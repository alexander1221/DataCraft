# Repository Guidelines

## Project Structure & Module Organization

This repository is currently an empty workspace: no source code, tests, assets, or build configuration are present. Establish the layout when introducing the first implementation, and update this guide to reflect actual paths.

Use `src/` for application code, `tests/` for automated tests, `assets/` for static resources, and `docs/` for supporting documentation when those directories become necessary. Organize modules by responsibility and keep related functionality together. Do not create empty directories solely to match this suggested layout.

## Build, Test, and Development Commands

No build, test, or local development commands are configured yet. When selecting the project tooling, document exact installation, run, test, and build commands in `README.md`. Include prerequisites and required runtime versions. Do not describe commands as supported until their configuration exists and they have been verified.

## Coding Style & Naming Conventions

Follow the conventions of the language and framework chosen for the implementation. Configure an appropriate formatter and linter early, and use consistent indentation throughout each file. Prefer descriptive module, function, and variable names. Keep generated files and local environment artifacts out of version control.

## Testing Guidelines

No testing framework or coverage threshold is established. Add tests alongside new behavior using the selected framework’s naming conventions. Cover normal operation, invalid inputs, and relevant edge cases. Document how contributors run the test suite before requiring checks in pull requests.

## Commit & Pull Request Guidelines

No Git history is available to establish existing commit conventions. Use concise, imperative commit subjects, such as `Add dataset import validation`, and keep commits focused.

Pull requests should describe the change, explain its purpose, and report validation performed. Link relevant issues and include screenshots for visible interface changes. Identify any setup steps, new dependencies, or known limitations.

## Security & Configuration

Never commit credentials, private datasets, or machine-specific configuration. Provide sanitized configuration examples when configuration is introduced.