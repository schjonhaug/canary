import { renderHook, waitFor, act } from '@testing-library/react';
import { useDashboard } from '../useDashboard';
import { useAuth } from '../../contexts/auth-context';
import { SSE } from 'sse.js';

// Mock the SSE library
jest.mock('sse.js', () => ({
  SSE: jest.fn(),
}));

// Mock the auth context
jest.mock('../../contexts/auth-context', () => ({
  useAuth: jest.fn(),
}));

// Mock the utils
jest.mock('../../lib/utils', () => ({
  getApiBaseUrl: jest.fn(() => 'http://localhost:3000'),
}));

// Mock fetch
global.fetch = jest.fn();

describe('useDashboard', () => {
  const mockSSE = SSE as jest.MockedClass<typeof SSE>;
  const mockUseAuth = useAuth as jest.MockedFunction<typeof useAuth>;
  const mockFetch = fetch as jest.MockedFunction<typeof fetch>;

  beforeEach(() => {
    jest.clearAllMocks();
    
    // Mock SSE instance
    const mockSSEInstance = {
      addEventListener: jest.fn(),
      stream: jest.fn(),
      close: jest.fn(),
    };
    mockSSE.mockImplementation(() => mockSSEInstance as unknown as SSE);
    
    // Mock fetch to return successful response
    mockFetch.mockResolvedValue({
      ok: true,
      json: async () => ({
        wallets: [],
        events: [],
        timestamp: Date.now(),
      }),
    } as Response);
  });

  it('should not connect to SSE when user is not authenticated', async () => {
    mockUseAuth.mockReturnValue({
      token: null,
      user: null,
      isLoading: false,
      isAuthenticated: false,
      login: jest.fn(),
      sendOtp: jest.fn(),
      logout: jest.fn(),
    });

    await act(async () => {
      renderHook(() => useDashboard());
    });

    expect(mockSSE).not.toHaveBeenCalled();
    expect(mockFetch).not.toHaveBeenCalled();
  });

  it('should not connect to SSE when no token is available', async () => {
    mockUseAuth.mockReturnValue({
      token: null,
      user: null,
      isLoading: false,
      isAuthenticated: true,
      login: jest.fn(),
      sendOtp: jest.fn(),
      logout: jest.fn(),
    });

    await act(async () => {
      renderHook(() => useDashboard());
    });

    expect(mockSSE).not.toHaveBeenCalled();
    expect(mockFetch).not.toHaveBeenCalled();
  });

  it('should connect to SSE with Authorization header when user is authenticated and has token', async () => {
    const mockToken = 'test-token';
    mockUseAuth.mockReturnValue({
      token: mockToken,
      user: { id: 1, phone_number: '+1234567890', is_admin: false },
      isLoading: false,
      isAuthenticated: true,
      login: jest.fn(),
      sendOtp: jest.fn(),
      logout: jest.fn(),
    });

    await act(async () => {
      renderHook(() => useDashboard());
    });

    await waitFor(() => {
      expect(mockSSE).toHaveBeenCalledWith(
        'http://localhost/api/dashboard/stream',
        {
          headers: {
            'Authorization': `Bearer ${mockToken}`,
          },
        }
      );
    });
  });

  it('should include Authorization header in initial data fetch when user is authenticated and has token', async () => {
    const mockToken = 'test-token';
    mockUseAuth.mockReturnValue({
      token: mockToken,
      user: { id: 1, phone_number: '+1234567890', is_admin: false },
      isLoading: false,
      isAuthenticated: true,
      login: jest.fn(),
      sendOtp: jest.fn(),
      logout: jest.fn(),
    });

    await act(async () => {
      renderHook(() => useDashboard());
    });

    await waitFor(() => {
      expect(mockFetch).toHaveBeenCalledWith(
        '/api/dashboard',
        {
          headers: {
            'Authorization': `Bearer ${mockToken}`,
          },
        }
      );
    });
  });

  it('should handle reconnection logic when connection fails', async () => {
    const mockToken = 'test-token';
    mockUseAuth.mockReturnValue({
      token: mockToken,
      user: { id: 1, phone_number: '+1234567890', is_admin: false },
      isLoading: false,
      isAuthenticated: true,
      login: jest.fn(),
      sendOtp: jest.fn(),
      logout: jest.fn(),
    });

    // Mock setTimeout to control reconnection timing
    jest.useFakeTimers();

    await act(async () => {
      renderHook(() => useDashboard());
    });

    // Wait for initial connection
    await waitFor(() => {
      expect(mockSSE).toHaveBeenCalledTimes(1);
    });

    // Simulate connection error
    const mockSSEInstance = mockSSE.mock.results[0].value;
    const errorListener = mockSSEInstance.addEventListener.mock.calls.find(
      call => call[0] === 'error'
    );
    
    if (errorListener) {
      await act(async () => {
        errorListener[1](new Event('error'));
      });
    }

    // Fast-forward time to trigger reconnection
    await act(async () => {
      jest.advanceTimersByTime(1000);
    });

    // Verify reconnection attempt
    await waitFor(() => {
      expect(mockSSE).toHaveBeenCalledTimes(2);
    });

    jest.useRealTimers();
  });
}); 