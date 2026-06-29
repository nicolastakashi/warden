"""Billing charge flow — BEFORE.

Trips:
  - no-cross-module-coupling (block): billing imports notifications directly,
    so a change to notification internals can break billing.
"""

from demo.before.notifications.email import send_receipt


def charge(order: dict) -> dict:
    result = {"status": "charged", "amount_cents": order["amount_cents"]}
    # BAD: billing reaching straight into notifications.
    send_receipt(order)
    return result
