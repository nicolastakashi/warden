"""Feature-flag config for the checkout service (unchanged from BEFORE)."""

FEATURE_FLAGS = {
    "express_checkout": False,
    "new_pricing": True,
}

# Config-driven settings the AFTER code reads instead of the environment.
SETTINGS = {
    "payment_gateway_url": "https://pay.example.com",
}


def get_flag(name: str, default: bool = False) -> bool:
    return FEATURE_FLAGS.get(name, default)


def get_setting(name: str) -> str:
    return SETTINGS[name]
