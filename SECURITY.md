# Security Policy

## Supported Versions

Security fixes are currently provided for the latest code on `main`.
Tagged releases may receive fixes at the maintainer's discretion, but only the newest release should be assumed supported.

| Version | Supported |
| ------- | --------- |
| `main`  | Yes       |
| Latest release | Yes |
| Older releases | No  |

## Reporting a Vulnerability

Please do not report security vulnerabilities in public GitHub issues.

Use one of these private channels instead:

1. GitHub Private Vulnerability Reporting, if it is enabled for this repository.
2. A direct private message to the maintainer with:
   - a clear description of the issue
   - reproduction steps or proof of concept
   - impact assessment
   - any suggested mitigation

## Response Expectations

- Initial acknowledgment target: within 7 days
- Status update target: within 14 days after acknowledgment
- Fix timing depends on severity, exploitability, and release impact

## Scope

This project shells out to system networking commands and may display sensitive local network information.
Please report issues involving:

- command execution or argument handling
- unsafe parsing of command output
- accidental disclosure of local or public network data
- release artifact integrity or supply chain concerns

## Operational Guidance

- Review screenshots and terminal recordings before sharing them publicly.
- Treat collected network details, routes, listening ports, and public IP information as potentially sensitive.
