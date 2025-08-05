import { renderHook, waitFor, act } from '@testing-library/react';
import { useDashboard } from '../useDashboard';
import { useAuth } from '../../contexts/auth-context';

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
  const mockUseAuth = useAuth as jest.MockedFunction<typeof useAuth>;
  const mockFetch = fetch as jest.MockedFunction<typeof fetch>;

  beforeEach(() => {
    jest.clearAllMocks();
    
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

  it('should not fetch dashboard data when user is not authenticated', async () => {
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

    expect(mockFetch).not.toHaveBeenCalled();
  });

  it('should not fetch dashboard data when no token is available', async () => {
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

    expect(mockFetch).not.toHaveBeenCalled();
  });

  it('should fetch dashboard data with Authorization header when user is authenticated and has token', async () => {
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

  it('should handle polling intervals correctly', async () => {
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

    // Mock timers to handle polling intervals
    jest.useFakeTimers();

    await act(async () => {
      renderHook(() => useDashboard());
    });

    // Initial fetch should happen immediately
    await waitFor(() => {
      expect(mockFetch).toHaveBeenCalledTimes(1);
    });

    // Fast-forward time by polling interval (60 seconds default)
    await act(async () => {
      jest.advanceTimersByTime(60000);
    });

    // Should trigger another fetch
    await waitFor(() => {
      expect(mockFetch).toHaveBeenCalledTimes(2);
    });

    jest.useRealTimers();
  });
}); 