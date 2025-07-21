import { getApiBaseUrl } from '@/lib/utils';

export async function GET() {
  const apiBaseUrl = getApiBaseUrl();
  const backendUrl = `${apiBaseUrl}/api/dashboard/stream`;

  try {
    const response = await fetch(backendUrl, {
      headers: {
        'Accept': 'text/event-stream',
        'Cache-Control': 'no-cache',
      },
    });

    if (!response.ok) {
      throw new Error(`Backend SSE failed: ${response.status}`);
    }

    // Create a readable stream that forwards the SSE data
    const readable = new ReadableStream({
      start(controller) {
        const reader = response.body?.getReader();
        if (!reader) {
          controller.error(new Error('No response body'));
          return;
        }

        const pump = async () => {
          try {
            const { done, value } = await reader.read();
            if (done) {
              controller.close();
              return;
            }
            controller.enqueue(value);
            pump();
          } catch (error) {
            controller.error(error);
          }
        };

        pump();
      },
    });

    return new Response(readable, {
      headers: {
        'Content-Type': 'text/event-stream',
        'Cache-Control': 'no-cache, no-transform',
        'Connection': 'keep-alive',
        'Access-Control-Allow-Origin': '*',
        'Access-Control-Allow-Headers': 'Cache-Control',
      },
    });
  } catch (error) {
    console.error('SSE Proxy Error:', error);
    return new Response('SSE proxy failed', { status: 500 });
  }
}