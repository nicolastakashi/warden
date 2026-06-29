"""Feature-flag config for the checkout service.

This file is fine — it *defines* the flag system. The warden wants callers to
use get_flag() rather than reaching into FEATURE_FLAGS directly.
"""

FEATURE_FLAGS = {
    "express_checkout": False,
    "new_pricing": True,
}


def get_flag(name: str, default: bool = False) -> bool:
    return FEATURE_FLAGS.get(name, default)
