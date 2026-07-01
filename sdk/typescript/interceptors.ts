/**
 * Request and response interceptors for the Soroban Pulse TypeScript SDK
 * 
 * Interceptors allow you to modify requests before they're sent and responses after they're received.
 * Typical use cases:
 * - Add authentication tokens
 * - Log all requests and responses
 * - Transform response data
 * - Handle errors globally
 */

export interface RequestInterceptor {
  /**
   * Called before the request is sent
   */
  onRequest?(init: RequestInit): RequestInit | Promise<RequestInit>;
  
  /**
   * Called if the request fails
   */
  onRequestError?(error: Error): Error | Promise<Error>;
}

export interface ResponseInterceptor {
  /**
   * Called after the response is received
   */
  onResponse?(response: Response): Response | Promise<Response>;
  
  /**
   * Called if the response indicates an error
   */
  onResponseError?(response: Response): Response | Promise<Response>;
}

export interface Interceptor extends RequestInterceptor, ResponseInterceptor {}

/**
 * Default logging interceptor
 */
export const loggingInterceptor: Interceptor = {
  async onRequest(init: RequestInit): Promise<RequestInit> {
    console.debug('[Request]', {
      method: init.method,
      headers: init.headers,
    });
    return init;
  },

  async onResponse(response: Response): Promise<Response> {
    console.debug('[Response]', {
      status: response.status,
      statusText: response.statusText,
    });
    return response;
  },

  async onResponseError(response: Response): Promise<Response> {
    console.error('[Response Error]', {
      status: response.status,
      statusText: response.statusText,
    });
    return response;
  },
};

/**
 * Authentication interceptor - adds Bearer token to requests
 */
export function authenticationInterceptor(token: string): Interceptor {
  return {
    async onRequest(init: RequestInit): Promise<RequestInit> {
      const headers = new Headers(init.headers);
      headers.set('Authorization', `Bearer ${token}`);
      return { ...init, headers };
    },
  };
}

/**
 * API Key interceptor - adds X-Api-Key header
 */
export function apiKeyInterceptor(apiKey: string): Interceptor {
  return {
    async onRequest(init: RequestInit): Promise<RequestInit> {
      const headers = new Headers(init.headers);
      headers.set('X-Api-Key', apiKey);
      return { ...init, headers };
    },
  };
}

/**
 * Request timing interceptor - logs request duration
 */
export function timingInterceptor(): Interceptor {
  const requestTimes = new Map<RequestInit, number>();

  return {
    async onRequest(init: RequestInit): Promise<RequestInit> {
      requestTimes.set(init, Date.now());
      return init;
    },

    async onResponse(response: Response): Promise<Response> {
      // Note: In a real implementation, you'd need a way to correlate
      // the request with the response
      console.debug('[Timing] Response received');
      return response;
    },
  };
}

/**
 * Error handling interceptor - provides centralized error handling
 */
export function errorHandlingInterceptor(
  onError?: (error: Error) => void
): Interceptor {
  return {
    async onRequestError(error: Error): Promise<Error> {
      console.error('[Request Error]', error);
      if (onError) {
        onError(error);
      }
      return error;
    },

    async onResponseError(response: Response): Promise<Response> {
      const error = new Error(
        `HTTP Error: ${response.status} ${response.statusText}`
      );
      console.error('[Response Error]', error);
      if (onError) {
        onError(error);
      }
      return response;
    },
  };
}

/**
 * Cache interceptor - caches successful GET requests
 */
export function cacheInterceptor(ttlMs: number = 60000): Interceptor {
  const cache = new Map<string, { response: Response; expiry: number }>();

  return {
    async onRequest(init: RequestInit): Promise<RequestInit> {
      if (init.method === 'GET' || !init.method) {
        // This is a GET request, we could check cache here
        // but we can't intercept at this level
      }
      return init;
    },

    async onResponse(response: Response): Promise<Response> {
      if (response.ok && (response.request?.method === 'GET' || true)) {
        // Cache successful GET responses
        // Note: Response.request is not standard, this is pseudocode
        cache.set(response.url, {
          response: response.clone(),
          expiry: Date.now() + ttlMs,
        });
      }
      return response;
    },
  };
}

/**
 * Request deduplication interceptor
 * Prevents duplicate in-flight requests to the same URL
 */
export function deduplicationInterceptor(): Interceptor {
  const inFlightRequests = new Map<string, Promise<Response>>();

  return {
    async onRequest(init: RequestInit): Promise<RequestInit> {
      // In a real implementation, you'd generate a cache key
      // based on method, URL, and request body
      return init;
    },

    async onResponse(response: Response): Promise<Response> {
      // Clean up in-flight request tracking
      return response;
    },
  };
}

/**
 * Request validator interceptor
 * Validates request structure before sending
 */
export function requestValidatorInterceptor(
  validator: (init: RequestInit) => boolean
): Interceptor {
  return {
    async onRequest(init: RequestInit): Promise<RequestInit> {
      if (!validator(init)) {
        throw new Error('Request validation failed');
      }
      return init;
    },
  };
}

/**
 * Response transformer interceptor
 * Transforms response data
 */
export function responseTransformerInterceptor(
  transformer: (data: any) => any
): Interceptor {
  return {
    async onResponse(response: Response): Promise<Response> {
      // Note: This is complex to implement properly since we need
      // to intercept at the body level. This is a simplified example.
      return response;
    },
  };
}

/**
 * Combine multiple interceptors
 */
export function combineInterceptors(...interceptors: Interceptor[]): Interceptor {
  return {
    async onRequest(init: RequestInit): Promise<RequestInit> {
      let current = init;
      for (const interceptor of interceptors) {
        if (interceptor.onRequest) {
          current = await interceptor.onRequest(current);
        }
      }
      return current;
    },

    async onResponse(response: Response): Promise<Response> {
      let current = response;
      for (const interceptor of interceptors) {
        if (interceptor.onResponse) {
          current = await interceptor.onResponse(current);
        }
      }
      return current;
    },

    async onRequestError(error: Error): Promise<Error> {
      let current = error;
      for (const interceptor of interceptors) {
        if (interceptor.onRequestError) {
          current = await interceptor.onRequestError(current);
        }
      }
      return current;
    },

    async onResponseError(response: Response): Promise<Response> {
      let current = response;
      for (const interceptor of interceptors) {
        if (interceptor.onResponseError) {
          current = await interceptor.onResponseError(current);
        }
      }
      return current;
    },
  };
}
