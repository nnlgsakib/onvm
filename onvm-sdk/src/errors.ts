export class OnvmError extends Error {
  public statusCode?: number;
  public details?: any;

  constructor(message: string, statusCode?: number, details?: any) {
    super(message);
    this.name = 'OnvmError';
    this.statusCode = statusCode;
    this.details = details;
    Object.setPrototypeOf(this, OnvmError.prototype);
  }

  get isAuthError(): boolean {
    return this.statusCode === 401 || this.statusCode === 403;
  }

  get isRateLimitError(): boolean {
    return this.statusCode === 429;
  }

  get isNetworkError(): boolean {
    return this.statusCode === undefined || (this.statusCode >= 500 && this.statusCode < 600);
  }
}

export class OnvmAuthError extends OnvmError {
  constructor(message: string, details?: any) {
    super(message, 401, details);
    this.name = 'OnvmAuthError';
    Object.setPrototypeOf(this, OnvmAuthError.prototype);
  }
}

export class OnvmNetworkError extends OnvmError {
  constructor(message: string, details?: any) {
    super(message, undefined, details);
    this.name = 'OnvmNetworkError';
    Object.setPrototypeOf(this, OnvmNetworkError.prototype);
  }
}

export class OnvmRateLimitError extends OnvmError {
  constructor(message: string, details?: any) {
    super(message, 429, details);
    this.name = 'OnvmRateLimitError';
    Object.setPrototypeOf(this, OnvmRateLimitError.prototype);
  }
}

export class OnvmConfigError extends OnvmError {
  constructor(message: string, details?: any) {
    super(message, undefined, details);
    this.name = 'OnvmConfigError';
    Object.setPrototypeOf(this, OnvmConfigError.prototype);
  }
}