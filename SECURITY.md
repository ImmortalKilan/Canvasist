# Security policy

Canvasist handles school sign-in sessions, so security reports are taken seriously.

## Reporting a vulnerability

Please **do not** open a public issue. Instead, use GitHub's private
[**Report a vulnerability**](../../security/advisories/new) form on this repository.

Include what you found, how to reproduce it, and its impact. You can expect an
acknowledgement within a few days and a fix or mitigation plan as soon as the
problem is understood.

Never include real session cookies, passwords, or other personal data in a
report; describe the issue with placeholders instead.

## Scope

In scope: the Canvasist desktop app and its build configuration in this
repository, for example leaks of session data, ways for web content to reach
Canvasist's internal commands, or unsafe storage of local data.

Out of scope: vulnerabilities in Canvas, Gradescope, your school's sign-in
system, Windows or WebView2 themselves. Please report those to their vendors.

## Supported versions

Only the latest release receives security fixes.
