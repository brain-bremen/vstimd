param(
    [Parameter(Position=0)]
    [string]$Target = "build"
)

$PROTO_SRC = @(
    "../../proto/vstimd/v1/color.proto",
    "../../proto/vstimd/v1/vec2.proto",
    "../../proto/vstimd/v1/system.proto",
    "../../proto/vstimd/v1/vtl.proto",
    "../../proto/vstimd/v1/animations.proto",
    "../../proto/vstimd/v1/stimuli/stimulus_type.proto",
    "../../proto/vstimd/v1/stimuli/shared_set_requests.proto",
    "../../proto/vstimd/v1/stimuli/shapes.proto",
    "../../proto/vstimd/v1/stimuli/rect.proto",
    "../../proto/vstimd/v1/stimuli/circle.proto",
    "../../proto/vstimd/v1/stimuli/ellipse.proto",
    "../../proto/vstimd/v1/stimuli/grating.proto",
    "../../proto/vstimd/v1/stimuli/text.proto",
    "../../proto/vstimd/v1/stimuli/polygon.proto",
    "../../proto/vstimd/v1/stimuli/query.proto",
    "../../proto/vstimd/v1/service.proto"
)
$PROTO_OUT = "vstimd/_proto"

function Invoke-Proto {
    uv run --group dev python -m grpc_tools.protoc `
        --proto_path=../../proto `
        "--python_out=$PROTO_OUT" `
        "--pyi_out=$PROTO_OUT" `
        @PROTO_SRC
}

switch ($Target) {
    "proto" {
        Invoke-Proto
    }
    "build" {
        Invoke-Proto
        uv build
    }
    "publish" {
        uv publish
    }
    "test" {
        Invoke-Proto
        uv pip install -r tests/unit/requirements-psychopy.txt
        uv pip install psychopy --no-deps
        uv run --group dev pytest tests/unit/
    }
    "test-integration" {
        Write-Host "Not implemented yet: integration tests with MockServer"
    }
    "test-e2e" {
        Invoke-Proto
        uv run --group dev pytest tests/e2e/test_e2e.py tests/e2e/test_psychopy_visual.py -v
    }
    "test-e2e-null" {
        Invoke-Proto
        uv run --group dev pytest tests/e2e/test_e2e_null.py tests/e2e/test_psychopy_visual_null.py -v
    }
    "docs" {
        Invoke-Proto
        uv run --group dev sphinx-build -b html docs docs/_build/html
        Write-Host "Docs built: docs/_build/html/index.html"
    }
    "clean" {
        Remove-Item -Recurse -Force -ErrorAction SilentlyContinue dist/, .venv/
    }
    default {
        Write-Error "Unknown target: $Target"
        Write-Host "Available targets: proto, build, publish, test, test-integration, test-e2e, test-e2e-null, docs, clean"
        exit 1
    }
}
