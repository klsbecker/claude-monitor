# Contribuir a Claude Monitor

[English](../../CONTRIBUTING.md) · [Português](../pt-BR/CONTRIBUTING.md) · **Español**

¡Gracias por ayudar! Los informes de errores, las ideas, las traducciones y el código son bienvenidos.

## Errores e ideas

Abre un [issue](https://github.com/klsbecker/claude-monitor/issues/new/choose) con las plantillas. En un error,
indica la versión de Windows, la versión de la app (menú → *Acerca de*), qué esperabas, qué pasó y las líneas
relevantes de `%LOCALAPPDATA%\ClaudeMonitor\claude-monitor.log`. **Nunca pegues el contenido de
`.credentials.json`** ni ningún token.

Los problemas de seguridad van por [SECURITY.md](../../SECURITY.md), no por issues públicos.

## Entorno de desarrollo

- Windows 10 u 11 y [rustup](https://rustup.rs). La versión del toolchain (con rustfmt y clippy) viene de
  `rust-toolchain.toml`.
- Con Visual Studio Build Tools (carga de trabajo C++), el toolchain MSVC por defecto compila con un simple
  `cargo build`.
- Sin ellas, `scripts\build-gnu.ps1` compila con el toolchain GNU. Necesita `dlltool` y `as`, por ejemplo de
  Strawberry Perl.

```powershell
cargo build                 # build de depuración
cargo run -- --demo         # datos inventados, sin iniciar sesión
cargo test
cargo fmt
cargo clippy --all-targets -- -D warnings
```

Ejecutar un build nuevo reemplaza la instancia abierta, así que puedes iterar sin cerrar la app a mano.

## Modelo de ramas (git flow)

| Rama | Para qué |
| --- | --- |
| `main` | Solo código publicado. Cada commit en `main` es una release con etiqueta `vX.Y.Z`. |
| `develop` | Integración de la próxima release. Los pull requests van a `develop`. |
| `feature/<nombre-corto>` | Funciones y correcciones no urgentes, creadas desde `develop`. |
| `release/X.Y.Z` | Preparación de la release (versión y changelog). Sale de `develop`, entra en `main` y vuelve a `develop`. |
| `hotfix/X.Y.Z` | Correcciones urgentes de una release. Sale de `main` y entra en `main` y en `develop`. |

Los mantenedores siguen [docs/releasing.md](../releasing.md) para publicar versiones.

## Versionado

El proyecto sigue el [Versionado Semántico](https://semver.org/lang/es/): `MAJOR.MINOR.PATCH`.

- **PATCH:** correcciones de errores y traducciones, sin cambios visibles de comportamiento.
- **MINOR:** funciones u opciones nuevas, compatibles con las anteriores (la configuración sigue valiendo).
- **MAJOR:** cambios incompatibles, como configuración o archivos que dejan de aprovecharse.

La versión está solo en `Cargo.toml`, y la app la muestra en el menú. Cada release tiene una entrada en
[CHANGELOG.md](../../CHANGELOG.md), en formato [Keep a Changelog](https://keepachangelog.com/es-ES/). En tu
pull request, añade el cambio en **Unreleased**.

## Commits

Usa [Conventional Commits](https://www.conventionalcommits.org/es/): `tipo: descripción`, en imperativo, en
minúsculas y sin punto final, **en inglés**. Tipos: `feat`, `fix`, `perf`, `refactor`, `docs`, `test`,
`build`, `ci`, `chore`, `style`.

```
feat: notify when the weekly limit resets
fix: keep the strip left of the notification area after explorer restarts
```

Explica el *porqué* en el cuerpo cuando no sea obvio. Cada commit tiene que compilar.

## Estándares de código

- **Todo en el código está en inglés:** identificadores, comentarios y mensajes de log. El texto que ve el
  usuario va en [`src/i18n.rs`](../../src/i18n.rs), nunca en medio del código.
- Formatea con `cargo fmt` (configurado en `rustfmt.toml`) y mantén `cargo clippy --all-targets -- -D warnings`
  sin avisos. El CI exige ambos.
- Un tema por módulo (consulta [docs/architecture.md](../architecture.md)). Mantén los bloques `unsafe`
  pequeños, solo alrededor de las llamadas Win32.
- Los comentarios de documentación (`///`) en elementos públicos dicen qué es o qué garantiza algo. Los
  comentarios normales explican el *porqué* (una peculiaridad de Windows, una restricción), no *qué* hace el
  código.
- Añade pruebas unitarias para la lógica pura (parsing, cálculos, formato). En cambios de interfaz, adjunta
  una captura al PR.
- No añadas dependencias sin un buen motivo: la app es un solo `.exe` pequeño.

## Traducciones

1. Añade una variante a `Lang` en `src/i18n.rs` y dale un código, un nombre nativo y el id de idioma de
   Windows en `Lang::system`.
2. Copia la tabla `EN`, traduce todas las cadenas (manteniendo los `{placeholders}`) y devuélvela en `tr()`.
3. Si quieres, traduce el README en `docs/<idioma>/README.md`.

## Pull requests

1. Haz un fork, crea la rama desde `develop` (`feature/...`), haz los commits y el push.
2. Abre el PR hacia `develop` y completa la plantilla.
3. El CI tiene que pasar (build, pruebas, fmt y clippy). Un mantenedor revisa y hace el merge.

Al contribuir, aceptas que tu trabajo queda bajo la [Licencia MIT](../../LICENSE).
