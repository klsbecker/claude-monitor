# Contribuindo com o Claude Monitor

[English](../../CONTRIBUTING.md) · **Português** · [Español](../es/CONTRIBUTING.md)

Obrigado pela ajuda! Relatos de bug, ideias, traduções e código são todos bem-vindos.

## Bugs e ideias

Abra uma [issue](https://github.com/klsbecker/claude-monitor/issues/new/choose) usando os modelos. Num bug,
informe a versão do Windows, a versão do app (menu → *Sobre*), o que você esperava, o que aconteceu e as linhas
relevantes de `%LOCALAPPDATA%\ClaudeMonitor\claude-monitor.log`. **Nunca cole o conteúdo do
`.credentials.json`** nem nenhum token.

Problemas de segurança vão pelo [SECURITY.md](../../SECURITY.md), não por issues públicas.

## Ambiente de desenvolvimento

- Windows 10 ou 11 e [rustup](https://rustup.rs). A versão do toolchain (com rustfmt e clippy) vem do
  `rust-toolchain.toml`.
- Com o Visual Studio Build Tools (workload C++), o toolchain MSVC padrão compila com um simples `cargo build`.
- Sem ele, o `scripts\build-gnu.ps1` compila com o toolchain GNU. Ele precisa de `dlltool` e `as`, que vêm por
  exemplo do Strawberry Perl.

```powershell
cargo build                 # build de debug
cargo run -- --demo         # dados inventados, sem login
cargo test
cargo fmt
cargo clippy --all-targets -- -D warnings
```

Rodar um build novo substitui a instância que está aberta, então dá para iterar sem fechar o app na mão.

## Modelo de branches (git flow)

| Branch | Para quê |
| --- | --- |
| `main` | Só código lançado. Todo commit na `main` é uma release com tag `vX.Y.Z`. |
| `develop` | Integração da próxima release. Os pull requests vão para a `develop`. |
| `feature/<nome-curto>` | Funcionalidades e correções não urgentes, criadas a partir da `develop`. |
| `release/X.Y.Z` | Preparação da release (versão e changelog). Sai da `develop`, entra na `main` e volta para a `develop`. |
| `hotfix/X.Y.Z` | Correções urgentes numa release. Sai da `main` e entra na `main` e na `develop`. |

Os mantenedores seguem o [docs/releasing.md](../releasing.md) para lançar versões.

## Versionamento

O projeto segue o [Versionamento Semântico](https://semver.org/lang/pt-BR/): `MAJOR.MINOR.PATCH`.

- **PATCH:** correções de bug e traduções, sem mudança visível de comportamento.
- **MINOR:** funcionalidades ou opções novas, compatíveis com as anteriores (as configurações continuam valendo).
- **MAJOR:** mudanças incompatíveis, como configurações ou arquivos que deixam de ser aproveitados.

A versão fica só no `Cargo.toml`, e o app a mostra no menu. Toda release tem uma entrada no
[CHANGELOG.md](../../CHANGELOG.md), no formato [Keep a Changelog](https://keepachangelog.com/pt-BR/). No seu
pull request, adicione a mudança em **Unreleased**.

## Commits

Use [Conventional Commits](https://www.conventionalcommits.org/pt-br/): `tipo: descrição`, no imperativo, em
minúsculas e sem ponto final, **em inglês**. Tipos: `feat`, `fix`, `perf`, `refactor`, `docs`, `test`,
`build`, `ci`, `chore`, `style`.

```
feat: notify when the weekly limit resets
fix: keep the strip left of the notification area after explorer restarts
```

Explique o *porquê* no corpo quando ele não for óbvio. Cada commit precisa compilar.

## Padrões de código

- **Tudo no código é em inglês:** identificadores, comentários e mensagens de log. Texto que o usuário vê fica no
  [`src/i18n.rs`](../../src/i18n.rs), nunca no meio do código.
- Formate com `cargo fmt` (configurado no `rustfmt.toml`) e mantenha `cargo clippy --all-targets -- -D warnings`
  sem avisos. O CI cobra os dois.
- Um assunto por módulo (veja o [docs/architecture.md](../architecture.md)). Mantenha os blocos `unsafe`
  pequenos, só em volta das chamadas Win32.
- Comentários de documentação (`///`) em itens públicos dizem o que a coisa é ou garante. Comentários comuns
  explicam o *porquê* (uma peculiaridade do Windows, uma restrição), não *o que* o código já diz.
- Adicione testes unitários para lógica pura (parsing, contas, formatação). Em mudanças de interface, anexe um
  print no PR.
- Não adicione dependências sem um bom motivo: o app é um único `.exe` pequeno.

## Traduções

1. Adicione uma variante em `Lang` no `src/i18n.rs` e dê a ela um código, um nome nativo e o id de idioma do
   Windows em `Lang::system`.
2. Copie a tabela `EN`, traduza todas as strings (mantendo os `{placeholders}`) e retorne a tabela em `tr()`.
3. Se quiser, traduza o README em `docs/<idioma>/README.md`.

## Pull requests

1. Faça um fork, crie a branch a partir da `develop` (`feature/...`), faça os commits e dê push.
2. Abra o PR para a `develop` e preencha o modelo.
3. O CI precisa passar (build, testes, fmt e clippy). Um mantenedor revisa e faz o merge.

Ao contribuir, você concorda que o seu trabalho fica sob a [Licença MIT](../../LICENSE).
