"""Receipt records — AFTER.

  - no-pii-in-logs: logs a non-identifying order id instead of name + email.
"""

import logging

logger = logging.getLogger(__name__)


def record_receipt(order_id: str, amount_cents: int) -> None:
    # FIXED: no PII — just the order id and amount.
    logger.info("Receipt for order %s: %d cents", order_id, amount_cents)
