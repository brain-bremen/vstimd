.PHONY: build publish test test-integration test-e2e clean

build:
	uv build

publish:
	uv publish

test:
	uv pip install -r tests/requirements-psychopy.txt
	uv pip install psychopy --no-deps
	uv run pytest tests/

test-integration:
	@echo "Not implemented yet: integration tests with MockServer"

test-e2e:
	@echo "Not implemented yet: e2e tests against a real wonderlamp_server"

clean:
	rm -rf dist/ .venv/
