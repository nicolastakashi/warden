"""Example config fixture.

Trips prefer-flag-helper (audit): direct FEATURE_FLAGS access instead of the
get_flag() helper. Audit means this is logged only — it never blocks and is
excluded from the score.
"""

FEATURE_FLAGS = {"new_checkout": True}


def new_checkout_enabled() -> bool:
    # AUDIT: prefer get_flag("new_checkout") over direct dict access.
    return FEATURE_FLAGS["new_checkout"]
