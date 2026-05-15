/**
 * Server envelope shape — every JSON response follows one of these forms.
 *
 * Success:    `{ status: "ok", data: T }`
 * Error:      `{ status: "error", error: { code, message } }`
 */

export interface ApiSuccess<T> {
  status: "ok";
  data: T;
}

export interface ApiErrorBody {
  status: "error";
  error: {
    code: string;
    message: string;
  };
}

export type ApiEnvelope<T> = ApiSuccess<T> | ApiErrorBody;

/**
 * Thrown by the API client when the server returns a non-2xx response with
 * the standard error envelope. Consumers branch on `.code` to surface
 * localised messages from `errors.yaml`.
 */
export class ApiError extends Error {
  readonly status: number;
  readonly code: string;

  constructor(status: number, code: string, message: string) {
    super(message);
    this.name = "ApiError";
    this.status = status;
    this.code = code;
  }
}

/**
 * Thrown when the request never reaches the server (offline, CORS, DNS) or
 * when the response body cannot be parsed as the expected envelope. The
 * dashboard treats this as "transient — retry button shown".
 */
export class NetworkError extends Error {
  readonly cause?: unknown;
  constructor(message: string, cause?: unknown) {
    super(message);
    this.name = "NetworkError";
    this.cause = cause;
  }
}
