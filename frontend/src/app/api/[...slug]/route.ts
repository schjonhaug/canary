import { NextRequest } from 'next/server';

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
  // Note: SSE endpoints are no longer supported - system uses polling instead
  if (slug.includes('stream')) {
    return new Response('Stream endpoints not supported - system uses polling', { status: 404 });
  }

  // Use API_URL for server-side, fallback to NEXT_PUBLIC_API_URL for compatibility
  const apiUrl = process.env.API_URL || process.env.NEXT_PUBLIC_API_URL;
  const backendUrl = apiUrl 
    ? `${apiUrl}/api/${slug.join('/')}`
    : `http://localhost:3000/api/${slug.join('/')}`;
  const url = new URL(backendUrl);

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

    // Forward Stripe webhook signature header
    const stripeSignature = request.headers.get('stripe-signature');
    if (stripeSignature) {
      headers['stripe-signature'] = stripeSignature;
    }

    let body: BodyInit | undefined;
    if (request.method !== 'GET' && request.method !== 'HEAD') {
      body = await request.text();
    }

    const response = await fetch(url.toString(), {
      method: request.method,
      headers,
      body,
    });

    // Copy response headers
    const responseHeaders = new Headers();
    response.headers.forEach((value, key) => {
      responseHeaders.set(key, value);
    });

    return new Response(response.body, {
      status: response.status,
      statusText: response.statusText,
      headers: responseHeaders,
    });
  } catch (error) {
    console.error('API Proxy Error:', error);
    console.error('Backend URL:', url.toString());
    
    // Return more detailed error information
    const errorMessage = error instanceof Error ? error.message : 'Unknown error';
    return new Response(JSON.stringify({ 
      error: 'Backend request failed',
      details: errorMessage,
      backendUrl: url.toString()
    }), { 
      status: 502,
      headers: { 'Content-Type': 'application/json' }
    });
  }
}