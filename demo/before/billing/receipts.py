"""Receipt records — BEFORE.

Trips:
  - no-pii-in-logs (warn, llm): logs the customer's full name and email address.
"""

import logging

logger = logging.getLogger(__name__)


def record_receipt(customer: dict, amount_cents: int) -> None:
    # BAD: logs PII (full name + email). The llm matcher flags this.
    logger.info(
        "Receipt for %s <%s>: %d cents",
        customer["full_name"],
        customer["email"],
        amount_cents,
    )
