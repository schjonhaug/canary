/**
 * @jest-environment node
 */
import { NextRequest } from 'next/server';
import { GET, POST, PUT, DELETE } from './route';

// Store original fetch so we can restore it
const originalFetch = global.fetch;

beforeEach(() => {
  jest.spyOn(console, 'error').mockImplementation(() => {});
});

afterEach(() => {
  global.fetch = originalFetch;
  jest.restoreAllMocks();
});

function makeRequest(
  method: string,
  path: string,
  options?: { headers?: Record<string, string>; body?: string }
) {
  const url = `http://localhost:3001/api/${path}`;
  const init: RequestInit = { method };
  if (options?.headers) init.headers = options.headers;
  if (options?.body) init.body = options.body;
  return new NextRequest(url, init);
}

function callHandler(method: string, request: NextRequest, slug: string[]) {
  const params = Promise.resolve({ slug });
  switch (method) {
    case 'GET': return GET(request, { params });
    case 'POST': return POST(request, { params });
    case 'PUT': return PUT(request, { params });
    case 'DELETE': return DELETE(request, { params });
    default: throw new Error(`Unknown method: ${method}`);
  }
}

function mockFetch(response: Partial<Response>) {
  const headers = new Headers(response.headers);
  global.fetch = jest.fn().mockResolvedValue({
    status: response.status ?? 200,
    statusText: response.statusText ?? 'OK',
    body: response.body ?? null,
    headers,
  });
  return global.fetch as jest.Mock;
}

describe('API Proxy Route', () => {
  describe('successful proxying', () => {
    it('forwards GET requests to the backend', async () => {
      const mock = mockFetch({ status: 200, headers: { 'content-type': 'application/json' } });

      const request = makeRequest('GET', 'wallets');
      const response = await callHandler('GET', request, ['wallets']);

      expect(response.status).toBe(200);
      expect(mock).toHaveBeenCalledWith(
        expect.stringContaining('/api/wallets'),
        expect.objectContaining({ method: 'GET', signal: expect.any(AbortSignal) })
      );
    });

    it('forwards POST body to the backend', async () => {
      const mock = mockFetch({ status: 201 });
      const body = JSON.stringify({ name: 'My Wallet' });

      const request = makeRequest('POST', 'wallets', {
        headers: { 'content-type': 'application/json' },
        body,
      });
      const response = await callHandler('POST', request, ['wallets']);

      expect(response.status).toBe(201);
      expect(mock).toHaveBeenCalledWith(
        expect.stringContaining('/api/wallets'),
        expect.objectContaining({ method: 'POST', body })
      );
    });

    it('forwards DELETE requests to the backend', async () => {
      const mock = mockFetch({ status: 200 });

      const request = makeRequest('DELETE', 'wallets/abc123/balance-alerts/1');
      const response = await callHandler('DELETE', request, ['wallets', 'abc123', 'balance-alerts', '1']);

      expect(response.status).toBe(200);
      expect(mock).toHaveBeenCalledWith(
        expect.stringContaining('/api/wallets/abc123/balance-alerts/1'),
        expect.objectContaining({ method: 'DELETE' })
      );
    });

    it('forwards authorization and cookie headers', async () => {
      const mock = mockFetch({ status: 200 });

      const request = makeRequest('GET', 'wallets', {
        headers: {
          authorization: 'Bearer token123',
          cookie: 'auth_token=abc',
        },
      });
      await callHandler('GET', request, ['wallets']);

      expect(mock).toHaveBeenCalledWith(
        expect.any(String),
        expect.objectContaining({
          headers: expect.objectContaining({
            authorization: 'Bearer token123',
            cookie: 'auth_token=abc',
          }),
        })
      );
    });

    it('forwards query parameters', async () => {
      const mock = mockFetch({ status: 200 });

      const request = makeRequest('GET', 'wallets?page=2&limit=10');
      await callHandler('GET', request, ['wallets']);

      const calledUrl = (mock.mock.calls[0] as string[])[0];
      expect(calledUrl).toContain('page=2');
      expect(calledUrl).toContain('limit=10');
    });
  });

  describe('stream endpoint blocking', () => {
    it('returns 404 for stream endpoints', async () => {
      const request = makeRequest('GET', 'wallets/stream');
      const response = await callHandler('GET', request, ['wallets', 'stream']);

      expect(response.status).toBe(404);
      const text = await response.text();
      expect(text).toContain('Stream endpoints not supported');
    });
  });

  describe('timeout handling', () => {
    it('returns 504 when the backend times out', async () => {
      const timeoutError = new DOMException('The operation was aborted due to timeout', 'TimeoutError');
      global.fetch = jest.fn().mockRejectedValue(timeoutError);

      const request = makeRequest('DELETE', 'wallets/abc/balance-alerts/1');
      const response = await callHandler('DELETE', request, ['wallets', 'abc', 'balance-alerts', '1']);

      expect(response.status).toBe(504);
      const json = await response.json();
      expect(json.error).toBe('Backend request timed out');
    });

    it('passes an AbortSignal to fetch', async () => {
      const mock = mockFetch({ status: 200 });

      const request = makeRequest('GET', 'wallets');
      await callHandler('GET', request, ['wallets']);

      const fetchOptions = mock.mock.calls[0][1];
      expect(fetchOptions.signal).toBeInstanceOf(AbortSignal);
    });
  });

  describe('error handling', () => {
    it('returns 502 when the backend is unreachable', async () => {
      global.fetch = jest.fn().mockRejectedValue(new Error('fetch failed'));

      const request = makeRequest('GET', 'wallets');
      const response = await callHandler('GET', request, ['wallets']);

      expect(response.status).toBe(502);
      const json = await response.json();
      expect(json.error).toBe('Backend request failed');
      expect(json.details).toBe('fetch failed');
    });

    it('returns 502 with unknown error for non-Error throws', async () => {
      global.fetch = jest.fn().mockRejectedValue('something weird');

      const request = makeRequest('GET', 'wallets');
      const response = await callHandler('GET', request, ['wallets']);

      expect(response.status).toBe(502);
      const json = await response.json();
      expect(json.details).toBe('Unknown error');
    });
  });
});
