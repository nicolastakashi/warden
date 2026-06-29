"""Checkout API — the AFTER version (every violation fixed).

  - no-env-vars       : reads from the config/flag system, not the environment
  - prefer-flag-helper: uses the get_flag helper instead of direct dict access
"""

from demo.after.config.flags import get_flag, get_setting


def payment_gateway_url() -> str:
    # FIXED: config-driven, no environment access.
    return get_setting("payment_gateway_url")


def express_checkout_enabled() -> bool:
    # FIXED: goes through the helper.
    return get_flag("express_checkout")
