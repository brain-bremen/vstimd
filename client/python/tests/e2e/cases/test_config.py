"""E2E tests for config persistence (ConfigClient)."""
from __future__ import annotations

import json

import pytest

from vstimd import (
    ConfigAlreadyExistsError,
    ConfigFormatError,
    ConfigNotFoundError,
    Connection,
)


def test_retrieve_returns_valid_json(conn: Connection) -> None:
    """retrieve() returns a non-empty string that parses as JSON."""
    raw = conn.config.retrieve()
    assert isinstance(raw, str) and len(raw) > 0
    data = json.loads(raw)
    assert data["version"] == 2
    assert "scene" in data
    assert "io" in data


def test_retrieve_scene_structure(conn: Connection) -> None:
    """retrieve() JSON contains expected scene keys."""
    data = json.loads(conn.config.retrieve())
    scene = data["scene"]
    assert "background" in scene
    assert "stimuli" in scene
    assert "animations" in scene


def test_upload_and_list(conn: Connection) -> None:
    """Uploaded config appears in list_configs()."""
    raw = conn.config.retrieve()
    conn.config.upload("e2e_test_list", raw, overwrite=True)
    names = conn.config.list_configs()
    assert "e2e_test_list" in names


def test_save_convenience(conn: Connection) -> None:
    """save() is equivalent to retrieve() + upload()."""
    conn.config.save("e2e_test_save", overwrite=True)
    assert "e2e_test_save" in conn.config.list_configs()


def test_upload_and_load_roundtrip(conn: Connection) -> None:
    """A config saved via upload() is restored correctly via load()."""
    # Create a rect, save config, delete everything, load back.
    h = conn.stimuli.shapes.create_rect(width=50, height=50, name="cfg_roundtrip_rect")
    conn.config.save("e2e_test_roundtrip", overwrite=True)
    conn.system.delete_all()

    stim_handles_before = {e.handle for e in conn.system.list_stimuli()}
    assert h not in stim_handles_before

    conn.config.load("e2e_test_roundtrip")
    entries = conn.system.list_stimuli()
    names = {e.name for e in entries}
    assert "cfg_roundtrip_rect" in names


def test_load_additive(conn: Connection) -> None:
    """load(additive=True) appends to the existing scene without clearing it."""
    conn.system.delete_all()
    h_existing = conn.stimuli.shapes.create_rect(name="existing_stim")

    conn.config.save("e2e_test_additive", overwrite=True)

    # Load the saved config additively (it contains "existing_stim").
    conn.config.load("e2e_test_additive", additive=True)

    names = {e.name for e in conn.system.list_stimuli()}
    # The original stimulus is still there AND the loaded one is added.
    assert "existing_stim" in names
    # Two entries named "existing_stim": the original and the loaded copy.
    count = sum(1 for e in conn.system.list_stimuli() if e.name == "existing_stim")
    assert count == 2

    conn.system.delete_all()


def test_upload_overwrite_false_raises(conn: Connection) -> None:
    """Uploading a config that already exists without overwrite=True raises."""
    raw = conn.config.retrieve()
    conn.config.upload("e2e_test_no_overwrite", raw, overwrite=True)
    with pytest.raises(ConfigAlreadyExistsError):
        conn.config.upload("e2e_test_no_overwrite", raw, overwrite=False)


def test_load_nonexistent_raises(conn: Connection) -> None:
    """Loading a config that does not exist raises ConfigNotFoundError."""
    with pytest.raises(ConfigNotFoundError):
        conn.config.load("this_name_does_not_exist_xyz123")


def test_upload_invalid_json_raises(conn: Connection) -> None:
    """Uploading a malformed JSON string raises ConfigFormatError."""
    with pytest.raises(ConfigFormatError):
        conn.config.upload("e2e_test_bad_json", "not valid json {{{", overwrite=True)


def test_upload_apply_now(conn: Connection) -> None:
    """upload(apply_now=True) applies the config immediately."""
    conn.system.delete_all()
    h = conn.stimuli.shapes.create_rect(name="apply_now_rect")
    raw = conn.config.retrieve()

    conn.system.delete_all()
    assert len(conn.system.list_stimuli()) == 0

    conn.config.upload("e2e_test_apply_now", raw, overwrite=True, apply_now=True)
    names = {e.name for e in conn.system.list_stimuli()}
    assert "apply_now_rect" in names

    conn.system.delete_all()
