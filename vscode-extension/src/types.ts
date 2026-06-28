export type HttpMethod = 'GET' | 'POST' | 'PUT' | 'DELETE' | 'PATCH';
export type AuthType = 'none' | 'api-key' | 'admin-key';

export interface EndpointParam {
    name: string;
    in: 'path' | 'query' | 'header';
    required: boolean;
    description?: string;
    example?: string;
}

export interface ApiEndpoint {
    method: HttpMethod;
    path: string;
    description: string;
    auth: AuthType;
    params?: EndpointParam[];
    bodyExample?: string;
    streaming?: boolean;
    deprecated?: boolean;
}

export interface EndpointGroup {
    label: string;
    endpoints: ApiEndpoint[];
}

export interface RequestConfig {
    method: HttpMethod;
    url: string;
    headers: Record<string, string>;
    body?: string;
}

export interface ResponseData {
    status: number;
    statusText: string;
    headers: Record<string, string>;
    body: string;
    durationMs: number;
    error?: string;
}

export type WebviewMessage =
    | { type: 'sendRequest'; config: RequestConfig }
    | { type: 'copyToClipboard'; text: string }
    | { type: 'openSettings' };

export type ExtensionMessage =
    | { type: 'response'; data: ResponseData }
    | { type: 'loading' }
    | { type: 'error'; message: string };
