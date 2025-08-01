export async function GET(request: Request) {
  const backendUrl = process.env.NEXT_PUBLIC_API_URL 
    ? `${process.env.NEXT_PUBLIC_API_URL}/api/dashboard/stream`
    : `http://localhost:3000/api/dashboard/stream`;

  try {
    // Get the Authorization header from the incoming request
    const authHeader = request.headers.get('authorization');
    
    const headers: HeadersInit = {
      'Accept': 'text/event-stream',
      'Cache-Control': 'no-cache',
    };

    // Add Authorization header if present
    if (authHeader) {
      headers['Authorization'] = authHeader;
    }

    const response = await fetch(backendUrl, {
      headers,
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
            
            // Check if controller is still open before enqueueing
            if (!controller.desiredSize) {
              // Controller is closed, stop pumping
              return;
            }
            
            controller.enqueue(value);
            pump();
          } catch (error) {
            console.error('SSE stream error:', error);
            // Only error if controller is still open
            if (controller.desiredSize !== null) {
              controller.error(error);
            }
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
        'Access-Control-Allow-Headers': 'Cache-Control, Authorization',
        'X-Accel-Buffering': 'no', // Disable nginx buffering
      },
    });
  } catch (error) {
    console.error('SSE Proxy Error:', error);
    return new Response('SSE proxy failed', { status: 500 });
  }
}