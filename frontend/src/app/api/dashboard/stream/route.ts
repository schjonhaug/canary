export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';
export const maxDuration = 300; // 5 minutes max duration for Vercel

// Handle unhandled rejections for SSE streams
if (typeof process !== 'undefined' && !process.listenerCount('unhandledRejection')) {
  process.on('unhandledRejection', (reason: any) => {
    if (
      reason?.name === 'ResponseAborted' ||
      reason?.name === 'AbortError' ||
      reason?.code === 'UND_ERR_BODY_TIMEOUT' ||
      reason?.message?.includes('aborted') ||
      reason?.message?.includes('terminated')
    ) {
      // These are expected when SSE connections close
      return;
    }
    // Re-throw other unhandled rejections
    throw reason;
  });
}

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

    // Create a more robust connection with retry logic
    let backendResponse: Response;
    let retryCount = 0;
    const maxRetries = 3;

    while (retryCount < maxRetries) {
      try {
        backendResponse = await fetch(backendUrl, {
          headers,
          // Add signal from request to properly handle aborts
          signal: request.signal,
        });

        if (backendResponse.ok) {
          break;
        } else {
          throw new Error(`Backend SSE failed: ${backendResponse.status}`);
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

    if (!backendResponse!.ok) {
      throw new Error(`Backend SSE failed: ${backendResponse!.status}`);
    }

    // Create a readable stream that forwards the SSE data
    const readable = new ReadableStream({
      start(controller) {
        const reader = backendResponse!.body?.getReader();
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
            if (!controller.desiredSize || controller.desiredSize === null) {
              // Controller is closed, stop pumping
              isClosed = true;
              reader.cancel().catch(() => {});
              return;
            }
            
            controller.enqueue(value);
            // Use setTimeout to avoid stack overflow and give event loop time to process
            setTimeout(() => {
              if (!isClosed) {
                pump();
              }
            }, 0);
          } catch (error: any) {
            // Ignore timeout errors, abort errors, and response aborted errors
            const isExpectedError = 
              error?.code === 'UND_ERR_BODY_TIMEOUT' || 
              error?.name === 'AbortError' ||
              error?.name === 'ResponseAborted' ||
              error?.message?.includes('terminated') ||
              error?.message?.includes('aborted');
              
            if (!isExpectedError) {
              console.error('SSE stream error:', error);
            }
            
            if (!isClosed) {
              try {
                controller.close();
              } catch (e) {
                // Controller already closed
              }
              isClosed = true;
            }
            reader.cancel().catch(() => {});
          }
        };

        // Start the pump
        pump().catch((error: any) => {
          // Handle pump errors
          if (
            error?.name !== 'ResponseAborted' &&
            error?.name !== 'AbortError' &&
            !error?.message?.includes('aborted')
          ) {
            console.error('Pump error:', error);
          }
        });

        // Handle client disconnect
        request.signal.addEventListener('abort', () => {
          console.log('Client disconnected from SSE');
          isClosed = true;
          reader.cancel().catch(() => {});
        });

        // Cleanup function
        const cleanup = () => {
          if (!isClosed) {
            isClosed = true;
            reader.cancel().catch(() => {});
            try {
              controller.close();
            } catch (e) {
              // Controller already closed
            }
          }
        };
      },
    });

    // Wrap the response to handle aborts gracefully
    const response = new Response(readable, {
      headers: {
        'Content-Type': 'text/event-stream',
        'Cache-Control': 'no-cache, no-transform',
        'Connection': 'keep-alive',
        'Access-Control-Allow-Origin': '*',
        'Access-Control-Allow-Headers': 'Cache-Control, Authorization',
        'X-Accel-Buffering': 'no', // Disable nginx buffering
      },
    });

    // Handle response abort to prevent unhandled rejections
    request.signal.addEventListener('abort', () => {
      // Response will be aborted automatically
    });

    return response;
  } catch (error: any) {
    // Don't log abort errors as they're expected
    if (error?.name !== 'AbortError' && !error?.message?.includes('aborted')) {
      console.error('SSE Proxy Error:', error);
    }
    return new Response('SSE proxy failed', { status: 500 });
  }
}