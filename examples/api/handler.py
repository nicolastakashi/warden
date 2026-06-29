"""Example API handler fixture.

Trips two rules:
  - no-env-vars (block)   : reads an environment variable directly
  - no-pii-in-logs (warn) : logs a user's email address (llm matcher)
"""

import logging
import os

logger = logging.getLogger(__name__)


def get_timeout() -> int:
    # BAD: direct env access — should use the feature-flag system.
    return int(os.getenv("REQUEST_TIMEOUT", "30"))


def handle(user_email: str) -> None:
    # BAD: logging PII (the user's email address).
    logger.info("Handling request for user %s", user_email)
