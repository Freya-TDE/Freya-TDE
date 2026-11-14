# Appendix C – How it was made

[Contents](README.md) · back: [Appendix B – Licenses](b-licenses.md)

Freya TDE, RUF and the CT-Editor were built **entirely with [Claude Code](https://claude.com/claude-code)**,
Anthropic's coding tool, using the model **Claude Opus 5.5**. Code, tests, packs, templates, icons and this
documentation were created in dialogue: the rights holder (Dragon Software) set the idea, requirements and
decisions and reviewed and tested every stage (the CT-Editor's ready-made packages also on Windows and
macOS); Claude Code designed, wrote, tested and documented.

## Guidelines

- **Core first, then the UI:** first the component model, dependency graph, pack format and code
  generation, then the code editor, the designer and the tools.
- **"Everything may, nothing must":** nothing is forced on the developer – own packs, own backends and own
  code stay untouched.
- **Always think of AI:** every function as a library API without UI, with machine-readable results, a
  preview before writing and stable codes – so that an AI can use the tool directly later.
- **Free licenses:** no paid or unclear dependencies.

The decisions made during development are recorded in [`CLAUDE.md`](../../CLAUDE.md) (German).
