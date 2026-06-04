PROTO_SRC  := ../../proto/vstimd/v1/vec2.proto \
              ../../proto/vstimd/v1/color.proto \
              ../../proto/vstimd/v1/stimuli/stimulus_type.proto \
              ../../proto/vstimd/v1/stimuli/shapes.proto \
              ../../proto/vstimd/v1/stimuli/query.proto \
              ../../proto/vstimd/v1/stimuli/rect.proto \
              ../../proto/vstimd/v1/stimuli/circle.proto \
              ../../proto/vstimd/v1/stimuli/ellipse.proto \
              ../../proto/vstimd/v1/stimuli/grating.proto \
              ../../proto/vstimd/v1/stimuli/text.proto \
              ../../proto/vstimd/v1/stimuli/polygon.proto \
              ../../proto/vstimd/v1/stimuli/shared_set_requests.proto \
              ../../proto/vstimd/v1/system.proto \
              ../../proto/vstimd/v1/vtl.proto \
              ../../proto/vstimd/v1/service.proto
PROTO_OUT  := vstimd/_proto

.PHONY: build publish proto test test-integration test-e2e test-e2e-null clean

proto:
	uv run --group dev python -m grpc_tools.protoc \
		--proto_path=../../proto \
		--python_out=$(PROTO_OUT) \
		--pyi_out=$(PROTO_OUT) \
		$(PROTO_SRC)

build: proto
	uv build

publish:
	uv publish

test: proto
	uv pip install -r tests/unit/requirements-psychopy.txt
	uv pip install psychopy --no-deps
	uv run --group dev pytest tests/unit/

test-integration:
	@echo "Not implemented yet: integration tests with MockServer"

test-e2e: proto
	uv run --group dev pytest tests/e2e/test_e2e.py tests/e2e/test_psychopy_visual.py -v

test-e2e-null: proto
	uv run --group dev pytest tests/e2e/test_e2e_null.py tests/e2e/test_psychopy_visual_null.py -v

clean:
	rm -rf dist/ .venv/
