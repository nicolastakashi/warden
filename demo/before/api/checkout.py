"""Checkout API — the BEFORE version (an AI agent wrote this in a hurry).

Trips:
  - no-env-vars (block)      : reads the gateway URL from the environment
  - prefer-flag-helper (audit): pokes FEATURE_FLAGS directly instead of get_flag()
"""

import os

from demo.before.config.flags import FEATURE_FLAGS


def payment_gateway_url() -> str:
    # BAD: should come from the flag/config system, not the environment.
    return os.getenv("PAYMENT_GATEWAY_URL", "https://pay.example.com")


def express_checkout_enabled() -> bool:
    # AUDIT: direct dict access — prefer get_flag("express_checkout").
    return FEATURE_FLAGS["express_checkout"]
