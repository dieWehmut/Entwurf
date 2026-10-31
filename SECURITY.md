# Security policy

Entwurf is an unreleased local-first prototype. There is no supported production release or guaranteed vulnerability response time yet.

Report security issues privately through [GitHub's private vulnerability reporting](https://github.com/dieWehmut/Entwurf/security/advisories/new) if that feature is enabled. If it is unavailable, request a private contact from the maintainer without publishing exploit details or personal data. Do not place textbooks, project databases, credentials, login state or unsanitized diagnostics in public issues.

Include the affected revision, operating system, minimal synthetic input, reproduction steps and observed impact. Identify any prerequisite local access or user action. Describe the boundary crossed, such as imported document to parser, bridge process to named pipe, model result to IR, or project file to export path.

Security requirements for v0.1 include local project data, no telemetry, explicit user initiation of GPT interaction, current-user pipe access with a session token, and safe pause on loss of foreground control. The bridge must not read cookies, call private APIs, bypass login or perform blind coordinate clicks. Model output and imported files remain untrusted inputs. These requirements guide development; they do not certify the current prototype as hardened.

Dependency vulnerabilities and missing release protections must be assessed against the runtime path actually shipped. Native installation, offline behavior, sidecar isolation and third-party license inventories remain release gates until verified on Windows.
