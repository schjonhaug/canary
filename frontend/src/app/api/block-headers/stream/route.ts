export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';
export const maxDuration = 300; // 5 minutes max duration for Vercel

// Handle unhandled rejections for SSE streams
if (typeof process !== 'undefined' && !process.listenerCount('unhandledRejection')) {
  process.on('unhandledRejection', (reason: unknown) => {
    const error = reason as Error & { code?: string };
    if (
      error?.name === 'ResponseAborted' ||
      error?.name === 'AbortError' ||
      error?.code === 'UND_ERR_BODY_TIMEOUT' ||
      error?.message?.includes('aborted') ||
      error?.message?.includes('terminated')
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
    ? `${process.env.NEXT_PUBLIC_API_URL}/api/block-headers/stream`
    : `http://localhost:3000/api/block-headers/stream`;

  try {
    // Create a more robust connection with retry logic
    let backendResponse: Response;
    let retryCount = 0;
    const maxRetries = 3;

    while (retryCount < maxRetries) {
      try {
        backendResponse = await fetch(backendUrl, {
          headers: {
            'Accept': 'text/event-stream',
            'Cache-Control': 'no-cache',
          },
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
              reader.cancel();
              return;
            }
            
            controller.enqueue(value);
            // Use setTimeout to avoid stack overflow and give event loop time to process
            setTimeout(() => {
              if (!isClosed) {
                pump();
              }
            }, 0);
          } catch (error) {
            // Ignore timeout errors, abort errors, and response aborted errors
            const err = error as Error & { code?: string };
            const isExpectedError = 
              err?.code === 'UND_ERR_BODY_TIMEOUT' || 
              err?.name === 'AbortError' ||
              err?.name === 'ResponseAborted' ||
              err?.message?.includes('terminated') ||
              err?.message?.includes('aborted');
              
            if (!isExpectedError) {
              console.error('SSE stream error:', error);
            }
            
            if (!isClosed) {
              try {
                controller.close();
              } catch {
                // Controller already closed
              }
              isClosed = true;
            }
            reader.cancel().catch(() => {});
          }
        };

        // Handle client disconnect
        request.signal.addEventListener('abort', () => {
          console.log('Client disconnected from SSE');
          isClosed = true;
          reader.cancel().catch(() => {});
        });

        // Cleanup function - commented out as it's not currently used
        // const cleanup = () => {
        //   if (!isClosed) {
        //     isClosed = true;
        //     reader.cancel().catch(() => {});
        //     try {
        //       controller.close();
        //     } catch {
        //       // Controller already closed
        //     }
        //   }
        // };

        // Start the pump
        pump().catch((error) => {
          // Handle pump errors
          const err = error as Error;
          if (
            err?.name !== 'ResponseAborted' &&
            err?.name !== 'AbortError' &&
            !err?.message?.includes('aborted')
          ) {
            console.error('Pump error:', error);
          }
        });
      },
    });

    // Wrap the response to handle aborts gracefully
    const response = new Response(readable, {
      headers: {
        'Content-Type': 'text/event-stream',
        'Cache-Control': 'no-cache, no-transform',
        'Connection': 'keep-alive',
        'Access-Control-Allow-Origin': '*',
        'Access-Control-Allow-Headers': 'Cache-Control',
        'X-Accel-Buffering': 'no', // Disable nginx buffering
      },
    });

    // Handle response abort to prevent unhandled rejections
    request.signal.addEventListener('abort', () => {
      // Response will be aborted automatically
    });

    return response;
  } catch (error) {
    // Don't log abort errors as they're expected
    const err = error as Error;
    if (err?.name !== 'AbortError' && !err?.message?.includes('aborted')) {
      console.error('SSE Proxy Error:', error);
    }
    return new Response('SSE proxy failed', { status: 500 });
  }
}