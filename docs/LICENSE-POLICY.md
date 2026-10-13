# Home Registry License Policy
# Last Updated: 2026-02-11

## Allowed Licenses

The following licenses are pre-approved for use in this project:

### Permissive Licenses (Preferred)
- MIT
- Apache-2.0
- BSD-2-Clause
- BSD-3-Clause
- ISC
- Zlib
- CC0-1.0
- Unlicense

### Weak Copyleft (Allowed with Review)
- MPL-2.0 (Mozilla Public License 2.0)
- LGPL-2.1-only (for runtime linking only)
- LGPL-3.0-only (for runtime linking only)

### Special Cases (Pre-Approved)
- OpenSSL (historical, for ring crate)
- Unicode-3.0

## Denied Licenses

The following licenses are NOT permitted:

### Strong Copyleft
- GPL-2.0-only
- GPL-2.0-or-later
- GPL-3.0-only
- GPL-3.0-or-later
- AGPL-3.0-only
- AGPL-3.0-or-later

### Non-Commercial / Restrictive
- CC-BY-NC-*
- SSPL
- BSL-1.1
- Proprietary

## Review Process

1. New dependencies must be checked with `cargo deny check licenses`
2. Frontend dependencies must be audited with `npm audit` and license-checker
3. Any license not in the allowed list requires explicit approval
4. Document exceptions in deny.toml with justification

## Compliance Verification

Run these commands before each release:

```bash
# Rust dependencies
cargo deny check licenses
cargo deny check advisories
cargo deny check bans

# Frontend dependencies
cd frontend
npx license-checker --summary
npm audit
```

## Contact

For license questions or exception requests, contact the project maintainers.
