import { NextRequest } from 'next/server';

// Ensure Node.js runtime for full Headers API support (including getSetCookie)
export const runtime = 'nodejs';

const MAX_REQUEST_BODY_SIZE = 1024 * 1024;

class RequestBodyTooLargeError extends Error {}

function limitRequestBody(body: ReadableStream<Uint8Array>) {
  let size = 0;

  return body.pipeThrough(new TransformStream<Uint8Array, Uint8Array>({
    transform(chunk, controller) {
      size += chunk.byteLength;
      if (size > MAX_REQUEST_BODY_SIZE) {
        controller.error(new RequestBodyTooLargeError());
        return;
      }

      controller.enqueue(chunk);
    },
  }));
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
      ? limitRequestBody(request.body)
      : undefined;

    const response = await fetch(url.toString(), {
      method: request.method,
      headers,
      body,
      // Node requires this option when forwarding a streaming request body.
      duplex: body ? 'half' : undefined,
      redirect: 'manual',
      signal: AbortSignal.timeout(30_000),
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
    const isTimeout = error instanceof DOMException && error.name === 'TimeoutError';

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
