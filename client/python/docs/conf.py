import sys
import os

sys.path.insert(0, os.path.abspath(".."))

project = "vstimd"
author = "Joscha Schmiedt"
release = "0.1.0"

extensions = [
    "sphinx.ext.autodoc",
    "sphinx.ext.napoleon",
    "sphinx_autodoc_typehints",
    "myst_parser",
    "sphinx_copybutton",
]

html_theme = "furo"
html_title = "vstimd"

autodoc_default_options = {
    "members": True,
    "undoc-members": True,
    "show-inheritance": True,
}
autodoc_typehints = "description"
autodoc_member_order = "bysource"
autodoc_type_aliases = {
    "StimulusHandle": "vstimd.StimulusHandle",
    "AnimationHandle": "vstimd.AnimationHandle",
}

myst_enable_extensions = ["colon_fence"]

exclude_patterns = ["_build"]
