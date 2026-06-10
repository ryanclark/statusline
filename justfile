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

build-signed developer_name team_id:
    cargo build --workspace --release --features codesigned
    codesign --force --options runtime --sign "Developer ID Application: {{developer_name}} ({{team_id}})" {{release_bin}}
    codesign --verify --verbose {{release_bin}}

install: build
    cp {{release_bin}} "${CARGO_HOME:-$HOME/.cargo}/bin/"

install-signed developer_name team_id: (build-signed developer_name team_id)
    cp {{release_bin}} "${CARGO_HOME:-$HOME/.cargo}/bin/"

dev *ARGS:
    cargo build --quiet
    {{target_bin}} {{ARGS}}

cert-request developer_name:
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
        echo "Clean up: just cert-clean"

cert-clean:
    rm -f devid.key devid.csr devid.crt devid.p12
