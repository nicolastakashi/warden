"""Billing charge flow — AFTER.

  - no-cross-module-coupling: billing no longer imports notifications. It returns
    an event for the caller (the API layer) to dispatch, decoupling the modules.
"""


def charge(order: dict) -> dict:
    return {
        "status": "charged",
        "amount_cents": order["amount_cents"],
        # Decoupled: emit an event instead of calling notifications directly.
        "events": [{"type": "receipt_requested", "order_id": order.get("id")}],
    }
