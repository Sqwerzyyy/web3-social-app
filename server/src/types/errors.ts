

export class AppError extends Error {
    constructor(
        public readonly code: ErrorCode,
        public readonly statusCode: number,
        message?: string
    ) {
        super(message ?? code);
        this.name = this.constructor.name;
        Object.setPrototypeOf(this, new.target.prototype);
    }
}

export class ValidationError extends AppError {
    constructor(code: ErrorCode, message?: string) {
        super(code, 400, message);
    }
}

export class AuthError extends AppError {

    constructor(code: ErrorCode, message?: string) {
        const statusCode = code === ErrorCode.FORBIDDEN ? 403 : 401;
        super(code, statusCode, message);
    }
}

export class ConflictError extends AppError {
    constructor(code: ErrorCode, message?: string) {
        super(code, 409, message);
    }
}

export class NotFoundError extends AppError {
    constructor(code: ErrorCode, message?: string) {
        super(code, 404, message);
    }
}

export enum ErrorCode {
    INTERNAL_SERVER_ERROR    = 'INTERNAL_SERVER_ERROR',
    VALIDATION_ERROR         = 'VALIDATION_ERROR',
    UNAUTHORIZED             = 'UNAUTHORIZED',
    FORBIDDEN                = 'FORBIDDEN',
    USER_NOT_FOUND           = 'USER_NOT_FOUND',
    EMAIL_ALREADY_EXISTS     = 'EMAIL_ALREADY_EXISTS',
    USERNAME_ALREADY_EXISTS  = 'USERNAME_ALREADY_EXISTS',
    INVALID_CREDENTIALS      = 'INVALID_CREDENTIALS',
    MISSING_REQUIRED_FIELDS  = 'MISSING_REQUIRED_FIELDS',
    INVALID_EMAIL            = 'INVALID_EMAIL',
    PASSWORD_TOO_SHORT       = 'PASSWORD_TOO_SHORT',
}

export const ERROR_HTTP_STATUS: Record<ErrorCode, number> = {
    [ErrorCode.INTERNAL_SERVER_ERROR]:   500,
    [ErrorCode.VALIDATION_ERROR]:        400,
    [ErrorCode.UNAUTHORIZED]:            401,
    [ErrorCode.FORBIDDEN]:               403,
    [ErrorCode.USER_NOT_FOUND]:          404,
    [ErrorCode.EMAIL_ALREADY_EXISTS]:    409,
    [ErrorCode.USERNAME_ALREADY_EXISTS]: 409,
    [ErrorCode.INVALID_CREDENTIALS]:     401,
    [ErrorCode.MISSING_REQUIRED_FIELDS]: 400,
    [ErrorCode.INVALID_EMAIL]:           400,
    [ErrorCode.PASSWORD_TOO_SHORT]:      400,
};

export const ERROR_MESSAGES: Record<ErrorCode, string> = {
    [ErrorCode.INTERNAL_SERVER_ERROR]:   'Internal server error',
    [ErrorCode.VALIDATION_ERROR]:        'Validation error',
    [ErrorCode.UNAUTHORIZED]:            'Authentication required',
    [ErrorCode.FORBIDDEN]:               'Access denied',
    [ErrorCode.USER_NOT_FOUND]:          'User not found',
    [ErrorCode.EMAIL_ALREADY_EXISTS]:    'This email is already registered',
    [ErrorCode.USERNAME_ALREADY_EXISTS]: 'This username is already taken',
    [ErrorCode.INVALID_CREDENTIALS]:     'Invalid email or password',
    [ErrorCode.MISSING_REQUIRED_FIELDS]: 'Missing required fields',
    [ErrorCode.INVALID_EMAIL]:           'Invalid email format',
    [ErrorCode.PASSWORD_TOO_SHORT]:      'Password must be at least 8 characters',
};