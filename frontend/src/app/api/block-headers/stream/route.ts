export async function GET() {
  const backendUrl = process.env.NEXT_PUBLIC_API_URL 
    ? `${process.env.NEXT_PUBLIC_API_URL}/api/block-headers/stream`
    : `http://localhost:3000/api/block-headers/stream`;

  try {
    // Create a more robust connection with retry logic
    let response: Response;
    let retryCount = 0;
    const maxRetries = 3;

    while (retryCount < maxRetries) {
      try {
        response = await fetch(backendUrl, {
          headers: {
            'Accept': 'text/event-stream',
            'Cache-Control': 'no-cache',
          },
          // Add timeout configuration
          signal: AbortSignal.timeout(300000), // 5 minutes timeout
        });

        if (response.ok) {
          break;
        } else {
          throw new Error(`Backend SSE failed: ${response.status}`);
        }
      } catch (error) {
        retryCount++;
        console.error(`SSE connection attempt ${retryCount} failed:`, error);
        
        if (retryCount >= maxRetries) {
          throw error;
        }
        
        // Wait before retrying
        await new Promise(resolve => setTimeout(resolve, 1000 * retryCount));
      }
    }

    if (!response!.ok) {
      throw new Error(`Backend SSE failed: ${response!.status}`);
    }

    // Create a readable stream that forwards the SSE data
    const readable = new ReadableStream({
      start(controller) {
        const reader = response!.body?.getReader();
        if (!reader) {
          controller.error(new Error('No response body'));
          return;
        }

        let isClosed = false;

        const pump = async () => {
          try {
            const { done, value } = await reader.read();
            if (done || isClosed) {
              if (!isClosed) {
                controller.close();
                isClosed = true;
              }
              return;
            }
            
            // Check if controller is still open before enqueueing
            if (!controller.desiredSize) {
              // Controller is closed, stop pumping
              isClosed = true;
              return;
            }
            
            controller.enqueue(value);
            pump();
          } catch (error) {
            console.error('SSE stream error:', error);
            if (!isClosed && controller.desiredSize !== null) {
              controller.error(error);
              isClosed = true;
            }
          }
        };

        // Start the pump
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
        'X-Accel-Buffering': 'no', // Disable nginx buffering
      },
    });
  } catch (error) {
    console.error('SSE Proxy Error:', error);
    return new Response('SSE proxy failed', { status: 500 });
  }
}