# Builds the release exe with the windows-gnu toolchain, for machines without Visual Studio Build Tools.
# The GNU toolchain needs dlltool and as; they are taken from Strawberry Perl (C:\Strawberry\c\bin), because
# older MinGW or MSYS2 runtimes on PATH break the build scripts.
param([string]$StrawberryBin = 'C:\Strawberry\c\bin')
$ErrorActionPreference = 'Stop'
$env:RUSTUP_TOOLCHAIN = '1.90.0-x86_64-pc-windows-gnu'
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:SystemRoot\system32;$env:SystemRoot;$StrawberryBin"
cargo build --release --manifest-path "$PSScriptRoot\..\Cargo.toml"
if ($LASTEXITCODE) { exit $LASTEXITCODE }
