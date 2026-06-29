"""Example billing fixture — intentionally clean.

Does NOT import notifications, so no-cross-module-coupling does not fire here.
(See tests/fixtures for a file that does trip it.)
"""


def charge(amount_cents: int) -> dict:
    return {"status": "charged", "amount_cents": amount_cents}
