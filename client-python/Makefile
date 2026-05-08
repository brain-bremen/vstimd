PROTO_SRC  := ../proto/wonderlamp.proto
PROTO_OUT  := wonderlamp/_proto

.PHONY: build publish proto test test-integration test-e2e test-e2e-null clean

proto:
	uv run python -m grpc_tools.protoc \
		--proto_path=../proto \
		--python_out=$(PROTO_OUT) \
		--pyi_out=$(PROTO_OUT) \
		$(PROTO_SRC)

build: proto
	uv build

publish:
	uv publish

test: proto
	uv pip install -r tests/requirements-psychopy.txt
	uv pip install psychopy --no-deps
	uv run pytest tests/

test-integration:
	@echo "Not implemented yet: integration tests with MockServer"

test-e2e: proto
	uv run pytest tests/test_e2e.py -v

test-e2e-null: proto
	uv run pytest tests/test_e2e_null.py -v

clean:
	rm -rf dist/ .venv/
