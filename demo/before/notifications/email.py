"""Notifications — clean target module (passes every rule)."""


def send_receipt(order: dict) -> None:
    # Pretend this enqueues an email. No policy issues here.
    print(f"queued receipt email for order {order.get('id', '?')}")
