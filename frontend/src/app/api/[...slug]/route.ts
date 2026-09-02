import { NextRequest } from 'next/server';

// Ensure Node.js runtime for full Headers API support (including getSetCookie)
export const runtime = 'nodejs';

const MAX_REQUEST_BODY_SIZE = 1024 * 1024;
const REQUEST_TIMEOUT_MS = 30_000;

class RequestBodyTooLargeError extends Error {}
class RequestBodyTimeoutError extends Error {}

async function readLimitedBody(body: ReadableStream<Uint8Array>) {
  let size = 0;
  const chunks: Uint8Array[] = [];
  const reader = body.getReader();
  const deadline = Date.now() + REQUEST_TIMEOUT_MS;

  try {
    while (true) {
      const { done, value } = await new Promise<ReadableStreamReadResult<Uint8Array>>((resolve, reject) => {
        const timeout = setTimeout(
          () => reject(new RequestBodyTimeoutError()),
          Math.max(0, deadline - Date.now())
        );
        reader.read().then(
          result => {
            clearTimeout(timeout);
            resolve(result);
          },
          error => {
            clearTimeout(timeout);
            reject(error);
          }
        );
      });
      if (done) break;

      size += value.byteLength;
      if (size > MAX_REQUEST_BODY_SIZE) {
        await reader.cancel();
        throw new RequestBodyTooLargeError();
      }

      chunks.push(value);
    }
  } catch (error) {
    if (error instanceof RequestBodyTimeoutError) {
      await reader.cancel();
    }
    throw error;
  } finally {
    reader.releaseLock();
  }

  const result = new Uint8Array(size);
  let offset = 0;
  for (const chunk of chunks) {
    result.set(chunk, offset);
    offset += chunk.byteLength;
  }

  return result;
}

export async function GET(
  request: NextRequest,
  { params }: { params: Promise<{ slug: string[] }> }
) {
  const { slug } = await params;
  return proxyToBackend(request, slug);
}

export async function POST(
  request: NextRequest,
  { params }: { params: Promise<{ slug: string[] }> }
) {
  const { slug } = await params;
  return proxyToBackend(request, slug);
}

export async function PUT(
  request: NextRequest,
  { params }: { params: Promise<{ slug: string[] }> }
) {
  const { slug } = await params;
  return proxyToBackend(request, slug);
}

export async function DELETE(
  request: NextRequest,
  { params }: { params: Promise<{ slug: string[] }> }
) {
  const { slug } = await params;
  return proxyToBackend(request, slug);
}

async function proxyToBackend(request: NextRequest, slug: string[]) {
  if (slug.includes('stream')) {
    return new Response('Stream endpoints not supported - system uses polling', { status: 404 });
  }

  // Use API_URL for server-side, fallback to NEXT_PUBLIC_API_URL for compatibility
  const apiUrl = process.env.API_URL || process.env.NEXT_PUBLIC_API_URL;
  const backendUrl = apiUrl 
    ? `${apiUrl}/api/${slug.join('/')}`
    : `http://localhost:3000/api/${slug.join('/')}`;
  const url = new URL(backendUrl);

  const contentLength = request.headers.get('content-length');
  if (contentLength && Number(contentLength) > MAX_REQUEST_BODY_SIZE) {
    return new Response(JSON.stringify({ error: 'Request body too large' }), {
      status: 413,
      headers: { 'Content-Type': 'application/json' },
    });
  }

  // Copy query parameters
  request.nextUrl.searchParams.forEach((value, key) => {
    url.searchParams.append(key, value);
  });

  try {
    const headers: HeadersInit = {};
    
    // Copy relevant headers
    const contentType = request.headers.get('content-type');
    if (contentType) {
      headers['content-type'] = contentType;
    }

    const authorization = request.headers.get('authorization');
    if (authorization) {
      headers['authorization'] = authorization;
    }

    const origin = request.headers.get('origin');
    if (origin) {
      headers['origin'] = origin;
    }

    const referer = request.headers.get('referer');
    if (referer) {
      headers['referer'] = referer;
    }

    const secFetchSite = request.headers.get('sec-fetch-site');
    if (secFetchSite) {
      headers['sec-fetch-site'] = secFetchSite;
    }

    // Forward cookies for HttpOnly auth token
    const cookie = request.headers.get('cookie');
    if (cookie) {
      headers['cookie'] = cookie;
    }

    // Forward Stripe webhook signature header
    const stripeSignature = request.headers.get('stripe-signature');
    if (stripeSignature) {
      headers['stripe-signature'] = stripeSignature;
    }

    const body = request.method !== 'GET' && request.method !== 'HEAD' && request.body
      ? await readLimitedBody(request.body)
      : undefined;

    const response = await fetch(url.toString(), {
      method: request.method,
      headers,
      body,
      redirect: 'manual',
      signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS),
    } as RequestInit);

    // Copy response headers, handling multiple Set-Cookie headers correctly
    const responseHeaders = new Headers();

    // Use getSetCookie() to properly handle multiple Set-Cookie headers
    // (forEach/entries may collapse them into one)
    // Fallback to empty array for safety in case getSetCookie is unavailable
    const setCookies = response.headers.getSetCookie?.() ?? [];
    for (const cookie of setCookies) {
      responseHeaders.append('Set-Cookie', cookie);
    }

    // Copy all other headers
    response.headers.forEach((value, key) => {
      if (key.toLowerCase() !== 'set-cookie') {
        responseHeaders.set(key, value);
      }
    });

    return new Response(response.body, {
      status: response.status,
      statusText: response.statusText,
      headers: responseHeaders,
    });
  } catch (error) {
    console.error('API Proxy Error:', error instanceof Error ? error.name : 'Unknown error');

    const isBodyTooLarge = error instanceof RequestBodyTooLargeError
      || (error instanceof Error && error.cause instanceof RequestBodyTooLargeError);
    const isTimeout = error instanceof RequestBodyTimeoutError
      || (error instanceof DOMException && error.name === 'TimeoutError');

    return new Response(JSON.stringify({
      error: isBodyTooLarge
        ? 'Request body too large'
        : isTimeout ? 'Backend request timed out' : 'Backend request failed',
    }), {
      status: isBodyTooLarge ? 413 : isTimeout ? 504 : 502,
      headers: { 'Content-Type': 'application/json' }
    });
  }
}
