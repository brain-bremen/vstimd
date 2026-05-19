"""Shared pytest configuration and fixtures for e2e tests."""

import os

import pytest
import zmq

from vstimd._proto import service_pb2, system_pb2

_E2E_DEFAULT = os.environ.get("VSTIMD_SERVER", "tcp://localhost:5555")


def pytest_addoption(parser: pytest.Parser) -> None:
    parser.addoption(
        "--server",
        default=_E2E_DEFAULT,
        help=f"ZMQ address of the vstimd for e2e tests (default: {_E2E_DEFAULT})",
    )
    parser.addoption(
        "--step-delay",
        type=float,
        default=1.0,
        help="Seconds to pause between visual stimulus changes so a human can inspect them (default: 1.0)",
    )


@pytest.fixture
def step_delay(request: pytest.FixtureRequest) -> float:
    return request.config.getoption("--step-delay")


def reachable(address: str, timeout_ms: int = 500) -> bool:
    ctx = zmq.Context.instance()
    sock = ctx.socket(zmq.REQ)
    sock.setsockopt(zmq.LINGER, 0)
    sock.setsockopt(zmq.RCVTIMEO, timeout_ms)
    sock.connect(address)
    try:
        req = service_pb2.Request(
            system=service_pb2.SystemTarget(),
            query_server_info=system_pb2.QueryServerInfoRequest(),
        )
        sock.send(req.SerializeToString())
        sock.recv()
        return True
    except zmq.Again:
        return False
    finally:
        sock.close()
