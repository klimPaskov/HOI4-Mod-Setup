# Open-source license decision

HOI4 Mod Setup needs a real `LICENSE` file before its first public source release. A public repository without an explicit license does not grant contributors and users the permissions expected from open-source software.

## Recommended decision

Choose one license after reviewing dependency compatibility and the desired contribution model.

| License | Useful when | Main consideration |
| --- | --- | --- |
| Apache License 2.0 | The project wants a permissive license with an explicit patent grant | Longer notice and attribution requirements |
| MIT License | The project wants a short permissive license | No explicit patent grant language |
| Mozilla Public License 2.0 | The project wants file-level copyleft while allowing larger combined works | Modified covered files must remain under MPL 2.0 |

The planning package does not choose a license on the maintainer's behalf.

## Release gate

Before the repository is described as released open source:

1. Add the selected license as `LICENSE` using the official unmodified text.
2. Update the `License` section in `README.md`.
3. Review all direct dependencies and bundled assets for compatible terms.
4. Add required third-party notices.
5. Include license and notice files in source and binary distributions.
6. Record the decision in an accepted architecture or governance note.
