"""Notifications — clean (unchanged from BEFORE)."""


def send_receipt(order: dict) -> None:
    print(f"queued receipt email for order {order.get('id', '?')}")
