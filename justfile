# Machine-specific settings (signing identity, etc) live in the gitignored justfile.local.
import? 'justfile.local'

bin := "statusline"
target_bin := "target" / "debug" / bin
release_bin := "target" / "release" / bin

default: build

gate: fmt lint test
    cargo build --workspace -q

check:
    cargo check --workspace --all-targets

test:
    cargo test --workspace

lint:
    cargo clippy --workspace --all-targets -- -D warnings
    cargo clippy -p statusline --all-targets --features codesigned -- -D warnings

fmt:
    cargo fmt --all
    taplo fmt

compile:
    cargo build --workspace

build:
    cargo build --workspace --release

# Recipe parameter backticks are the only expressions that can see the exports from justfile.local, so the signing
# identity defaults are read there rather than in top-level variables.

# Codesign with the given identity, or with DEVELOPER_NAME and TEAM_ID from justfile.local
build-signed developer_name=`echo "${DEVELOPER_NAME:-}"` team_id=`echo "${TEAM_ID:-}"`: (require-identity developer_name team_id)
    cargo build --workspace --release --features codesigned
    codesign --force --options runtime --sign "Developer ID Application: {{developer_name}} ({{team_id}})" {{release_bin}}
    codesign --verify --verbose {{release_bin}}

install: build
    cp {{release_bin}} "${CARGO_HOME:-$HOME/.cargo}/bin/"

# Codesigned build copied into CARGO_HOME; takes the same identity defaults as build-signed
install-signed developer_name=`echo "${DEVELOPER_NAME:-}"` team_id=`echo "${TEAM_ID:-}"`: (build-signed developer_name team_id)
    cp {{release_bin}} "${CARGO_HOME:-$HOME/.cargo}/bin/"

[private]
require-identity developer_name team_id:
    @test -n "{{developer_name}}" -a -n "{{team_id}}" || { echo "pass developer_name and team_id, or export DEVELOPER_NAME and TEAM_ID in justfile.local" >&2; exit 1; }

dev *ARGS:
    cargo build --quiet
    {{target_bin}} {{ARGS}}

cert-request developer_name=`echo "${DEVELOPER_NAME:-}"`:
    @test -n "{{developer_name}}" || { echo "pass developer_name, or export DEVELOPER_NAME in justfile.local" >&2; exit 1; }
    openssl req -new -newkey rsa:2048 -nodes \
        -keyout devid.key -out devid.csr \
        -subj "/CN={{developer_name}}"
    @echo ""
    @echo "Upload devid.csr at:"
    @echo "  https://developer.apple.com/account/resources/certificates/add"
    @echo "Select 'Developer ID Application', then download the .cer file."
    @echo ""
    @echo "Then run: just cert-import <path-to-downloaded.cer>"

cert-import cer:
    openssl x509 -inform DER -in "{{cer}}" -out devid.crt
    openssl pkcs12 -export -out devid.p12 -inkey devid.key -in devid.crt -legacy
    security import devid.p12 -k ~/Library/Keychains/login.keychain-db
    @echo ""
    @echo "Certificate imported. Your signing identity:"
    @security find-identity -v -p codesigning | grep "Developer ID Application"
    @echo ""
    @IDENTITY=$(security find-identity -v -p codesigning | grep "Developer ID Application" | head -1 | sed 's/.*"\(.*\)".*/\1/'); \
        NAME=$(echo "$IDENTITY" | sed 's/Developer ID Application: \(.*\) (.*/\1/'); \
        TEAM=$(echo "$IDENTITY" | sed 's/.*(\(.*\))/\1/'); \
        echo "Test with:"; \
        echo "  just install-signed \"$NAME\" \"$TEAM\""; \
        echo ""; \
        echo "Or save the identity for future builds in justfile.local:"; \
        echo "  export DEVELOPER_NAME := \"$NAME\""; \
        echo "  export TEAM_ID := \"$TEAM\""; \
        echo ""; \
        echo "Clean up: just cert-clean"

cert-clean:
    rm -f devid.key devid.csr devid.crt devid.p12
