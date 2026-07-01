# coding: utf-8

"""
REST client with enhanced retry and backoff configuration for Soroban Pulse SDK

This module provides retry policy integration for the REST client.
"""

import asyncio
import io
import json
import logging
from typing import Optional

import httpx

from openapi_client.exceptions import ApiException, ApiValueError
from openapi_client.retry_policy import (
    RetryPolicy,
    create_default_retry_policy,
    get_global_retry_policy,
)

logger = logging.getLogger(__name__)

RESTResponseType = httpx.Response


class RESTResponse(io.IOBase):

    def __init__(self, resp) -> None:
        self.response = resp
        self.status = resp.status_code
        self.reason = resp.reason_phrase
        self.data = None

    async def read(self):
        if self.data is None:
            self.data = await self.response.aread()
        return self.data

    @property
    def headers(self):
        """Returns response headers."""
        return self.response.headers

    def getheaders(self):
        """Returns response headers; use ``headers`` instead."""
        return self.response.headers

    def getheader(self, name, default=None):
        """Returns a given response header; use ``headers`` instead."""
        return self.response.headers.get(name, default)


class RESTClientWithRetry:
    """REST client with integrated retry policy support"""

    def __init__(self, configuration) -> None:
        # Connection settings
        self.maxsize = configuration.connection_pool_maxsize
        
        # Legacy retry settings (for backwards compatibility)
        self.max_retries = getattr(configuration, 'max_retries', 3)
        self.retry_on_status = getattr(configuration, 'retry_on_status', [429, 500, 502, 503, 504])

        # New retry policy support
        retry_policy_config = getattr(configuration, 'retry_policy_config', None)
        if retry_policy_config:
            self.retry_policy = RetryPolicy(retry_policy_config)
        else:
            # Use global retry policy if not configured
            self.retry_policy = get_global_retry_policy()
            # Update to use legacy settings if available
            config = self.retry_policy.get_config()
            config.max_retries = self.max_retries
            config.retryable_status_codes = set(self.retry_on_status)
            self.retry_policy.update_config(config)

        # SSL settings
        import ssl
        self.ssl_context = ssl.create_default_context(
            cafile=configuration.ssl_ca_cert,
            cadata=configuration.ca_cert_data,
        )
        if configuration.cert_file:
            self.ssl_context.load_cert_chain(
                configuration.cert_file, keyfile=configuration.key_file
            )

        if not configuration.verify_ssl:
            self.ssl_context.check_hostname = False
            self.ssl_context.verify_mode = ssl.CERT_NONE

        self.proxy = configuration.proxy
        self.proxy_headers = configuration.proxy_headers
        self.pool_manager: Optional[httpx.AsyncClient] = None

    async def close(self):
        """Close the connection pool"""
        if self.pool_manager is not None:
            await self.pool_manager.aclose()

    async def request(
        self,
        method,
        url,
        headers=None,
        body=None,
        post_params=None,
        _request_timeout=None,
    ):
        """
        Execute HTTP request with retry logic

        :param method: HTTP request method
        :param url: HTTP request URL
        :param headers: HTTP request headers
        :param body: Request JSON body for `application/json`
        :param post_params: Request POST parameters
        :param _request_timeout: Timeout setting for this request
        :return: RESTResponse object
        """
        import re
        
        method = method.upper()
        assert method in ['GET', 'HEAD', 'DELETE', 'POST', 'PUT', 'PATCH', 'OPTIONS']

        if post_params and body:
            raise ApiValueError(
                "body parameter cannot be used with post_params parameter."
            )

        post_params = post_params or {}
        headers = headers or {}
        timeout = _request_timeout or 5 * 60

        if 'Content-Type' not in headers:
            headers['Content-Type'] = 'application/json'

        args = {
            "method": method,
            "url": url,
            "timeout": timeout,
            "headers": headers
        }

        # Prepare request body based on content type
        if method in ['POST', 'PUT', 'PATCH', 'OPTIONS', 'DELETE']:
            if re.search('json', headers['Content-Type'], re.IGNORECASE):
                if body is not None:
                    args["json"] = body
            elif headers['Content-Type'] == 'application/x-www-form-urlencoded':
                args["data"] = dict(post_params)
            elif headers['Content-Type'] == 'multipart/form-data':
                del headers['Content-Type']
                files = []
                data = {}
                for param in post_params:
                    k, v = param
                    if isinstance(v, tuple) and len(v) == 3:
                        files.append((k, v))
                    else:
                        if isinstance(v, dict):
                            v = json.dumps(v)
                        elif isinstance(v, int):
                            v = str(v)
                        data[k] = v
                if files:
                    args["files"] = files
                if data:
                    args["data"] = data
            elif isinstance(body, (str, bytes)):
                args["data"] = body
            else:
                msg = """Cannot prepare a request message for provided arguments.
                         Please check that your arguments match declared content type."""
                raise ApiException(status=0, reason=msg)

        if self.pool_manager is None:
            self.pool_manager = self._create_pool_manager()

        # Execute request with retry logic
        request_key = f"{method}:{url}"
        
        for attempt in range(self.retry_policy.config.max_retries + 1):
            try:
                response = await self.pool_manager.request(**args)
                
                # Check if request succeeded
                if response.status_code < 300:  # Success
                    self.retry_policy.record_success(request_key, attempt)
                    return RESTResponse(response)
                
                # Check if we should retry this status code
                if response.status_code not in self.retry_policy.config.retryable_status_codes:
                    # Status code is not retryable
                    return RESTResponse(response)
                
                # Check if we've exhausted retries
                if attempt >= self.retry_policy.config.max_retries:
                    # No more retries
                    return RESTResponse(response)
                
                # Calculate delay
                retry_after = response.headers.get("retry-after") or response.headers.get("Retry-After")
                delay_seconds = self.retry_policy.get_delay_seconds(attempt, retry_after)
                
                # Record retry
                reason = f"HTTP {response.status_code}"
                self.retry_policy.record_retry(request_key, attempt, delay_seconds, reason)
                
                # Wait before retrying
                await asyncio.sleep(delay_seconds)
                
            except (ConnectionError, TimeoutError, OSError) as e:
                # Check if we should retry this error
                if not self.retry_policy.should_retry(e, attempt):
                    raise
                
                if attempt >= self.retry_policy.config.max_retries:
                    raise
                
                # Calculate delay
                delay_seconds = self.retry_policy.get_delay_seconds(attempt)
                reason = f"{type(e).__name__}: {str(e)}"
                self.retry_policy.record_retry(request_key, attempt, delay_seconds, reason)
                
                # Wait before retrying
                await asyncio.sleep(delay_seconds)

        # This should not be reached, but just in case
        raise ApiException(status=0, reason="Request failed after all retries")

    def set_default_header(self, header_name, header_value):
        """Set default HTTP header"""
        if self.pool_manager is None:
            self.pool_manager = self._create_pool_manager()
        # Note: httpx.AsyncClient doesn't have set_default_header,
        # so this is a no-op for now. Headers should be passed directly.
        pass

    def _create_pool_manager(self):
        """Create and configure httpx.AsyncClient"""
        return httpx.AsyncClient(
            limits=httpx.Limits(
                max_connections=self.maxsize,
                max_keepalive_connections=self.maxsize,
            ),
            verify=self.ssl_context,
            proxy=self.proxy,
            headers=self.proxy_headers,
        )


# Backward compatibility: expose original RESTClientObject as well
RESTClientObject = RESTClientWithRetry
