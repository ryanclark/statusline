DEVELOPER_NAME ?=
TEAM_ID ?=

.PHONY: build build-signed install install-signed dev cert-request cert-import cert-clean

build:
	cargo build --release

build-signed:
ifndef DEVELOPER_NAME
	$(error DEVELOPER_NAME is required)
endif
ifndef TEAM_ID
	$(error TEAM_ID is required)
endif
	cargo build --release --features codesigned
	codesign --force --options runtime --sign "Developer ID Application: $(DEVELOPER_NAME) ($(TEAM_ID))" target/release/statusline
	codesign --verify --verbose target/release/statusline

install: build
	cp target/release/statusline $$(cargo home 2>/dev/null || echo $$HOME/.cargo)/bin/

install-signed: build-signed
	cp target/release/statusline $$(cargo home 2>/dev/null || echo $$HOME/.cargo)/bin/

dev:
	cargo build --quiet 2>/dev/null
	exec ./target/debug/statusline $(ARGS)

cert-request:
ifndef DEVELOPER_NAME
	$(error DEVELOPER_NAME is required)
endif
	openssl req -new -newkey rsa:2048 -nodes \
		-keyout devid.key -out devid.csr \
		-subj "/CN=$(DEVELOPER_NAME)"
	@echo ""
	@echo "Upload devid.csr at:"
	@echo "  https://developer.apple.com/account/resources/certificates/add"
	@echo "Select 'Developer ID Application', then download the .cer file."
	@echo ""
	@echo "Then run: make cert-import CER=<path-to-downloaded.cer>"

cert-import:
ifndef CER
	$(error CER is required)
endif
	openssl x509 -inform DER -in "$(CER)" -out devid.crt
	openssl pkcs12 -export -out devid.p12 -inkey devid.key -in devid.crt -legacy
	security import devid.p12 -k ~/Library/Keychains/login.keychain-db
	@echo ""
	@echo "Certificate imported. Your signing identity:"
	@security find-identity -v -p codesigning | grep "Developer ID Application"
	@echo ""
	@IDENTITY=$$(security find-identity -v -p codesigning | grep "Developer ID Application" | head -1 | sed 's/.*"\(.*\)".*/\1/'); \
		NAME=$$(echo "$$IDENTITY" | sed 's/Developer ID Application: \(.*\) (.*/\1/'); \
		TEAM=$$(echo "$$IDENTITY" | sed 's/.*(\(.*\))/\1/'); \
		echo "Test with:"; \
		echo "  make install-signed DEVELOPER_NAME=\"$$NAME\" TEAM_ID=\"$$TEAM\""; \
		echo ""; \
		echo "Clean up: make cert-clean"

cert-clean:
	rm -f devid.key devid.csr devid.crt devid.p12
