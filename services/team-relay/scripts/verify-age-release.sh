#!/usr/bin/env bash
set -euo pipefail

fail() {
  printf 'age release verification failed: %s\n' "$*" >&2
  exit 1
}

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
signer_keys="$root/age-sigsum-key.pub"
version="${CNSHELL_AGE_VERSION:-v1.3.1}"
verify_bin="${CNSHELL_SIGSUM_VERIFY_BIN:-$(command -v sigsum-verify 2>/dev/null || true)}"

[[ "$version" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]] || fail "version must use vMAJOR.MINOR.PATCH"
case "$version" in
  v1.2.1 | v1.3.1) ;;
  *) fail "age version $version has not been reviewed for this verifier" ;;
esac
[[ -x "$verify_bin" ]] || fail "sigsum-verify v0.13.1 is required"
[[ "$("$verify_bin" --version)" == "sigsum-verify (sigsum-go module) v0.13.1" ]] || \
  fail "sigsum-verify must be exactly v0.13.1"
[[ -f "$signer_keys" ]] || fail "age signer key file is missing"

platform="${CNSHELL_AGE_PLATFORM:-}"
if [[ -z "$platform" ]]; then
  case "$(uname -s)/$(uname -m)" in
    Darwin/arm64) platform="darwin/arm64" ;;
    Darwin/x86_64) platform="darwin/amd64" ;;
    Linux/aarch64 | Linux/arm64) platform="linux/arm64" ;;
    Linux/x86_64 | Linux/amd64) platform="linux/amd64" ;;
    *) fail "unsupported host platform $(uname -s)/$(uname -m)" ;;
  esac
fi
case "$platform" in
  darwin/arm64 | darwin/amd64 | linux/arm64 | linux/amd64) ;;
  *) fail "unsupported age platform $platform" ;;
esac

[[ $# -eq 1 ]] || fail "usage: verify-age-release.sh ABSOLUTE_OUTPUT_DIRECTORY"
destination="$1"
[[ "$destination" == /* ]] || fail "output directory must be absolute"
[[ ! -e "$destination" && ! -L "$destination" ]] || fail "output directory already exists"

temporary_directory="$(mktemp -d "${TMPDIR:-/tmp}/cnshell-age-release.XXXXXX")"
completed=false
cleanup() {
  rm -rf "$temporary_directory"
  if [[ "$completed" != true ]]; then
    rm -rf "$destination"
  fi
}
trap cleanup EXIT INT TERM
mkdir "$destination"

platform_name="${platform//\//-}"
artifact="age-$version-$platform_name.tar.gz"
archive="$temporary_directory/$artifact"
proof="$archive.proof"
base_url="https://dl.filippo.io/age/$version?for=$platform"

curl --fail --location --silent --show-error --retry 3 --output "$archive" "$base_url"
curl --fail --location --silent --show-error --retry 3 --output "$proof" "$base_url&proof"
"$verify_bin" -k "$signer_keys" -P sigsum-generic-2025-1 "$proof" < "$archive"

case "$version" in
  v1.2.1)
    expected_listing=$'age/\nage/LICENSE\nage/age-keygen\nage/age'
    ;;
  v1.3.1)
    expected_listing=$'age/\nage/LICENSE\nage/age-inspect\nage/age-plugin-batchpass\nage/age\nage/age-keygen'
    ;;
esac
actual_listing="$(tar -tzf "$archive")"
[[ "$actual_listing" == "$expected_listing" ]] || fail "release archive contains unexpected entries"
tar -xzf "$archive" -C "$destination"

age_bin="$destination/age/age"
age_keygen_bin="$destination/age/age-keygen"
[[ -f "$age_bin" && ! -L "$age_bin" && -x "$age_bin" ]] || fail "verified archive did not contain the age executable"
[[ -f "$age_keygen_bin" && ! -L "$age_keygen_bin" && -x "$age_keygen_bin" ]] || fail "verified archive did not contain the age-keygen executable"
[[ "$("$age_bin" --version)" == "$version" ]] || fail "age binary version does not match archive"
[[ "$("$age_keygen_bin" --version)" == "$version" ]] || fail "age-keygen version does not match archive"

if command -v sha256sum >/dev/null 2>&1; then
  archive_sha256="$(sha256sum "$archive" | awk '{print $1}')"
else
  archive_sha256="$(shasum -a 256 "$archive" | awk '{print $1}')"
fi
completed=true
printf 'age release verified: %s\n' "$version"
printf 'platform: %s\n' "$platform"
printf 'archive SHA-256: %s\n' "$archive_sha256"
printf 'tools: %s\n' "$destination/age"
