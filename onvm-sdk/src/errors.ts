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
