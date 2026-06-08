# Adapter Gallery

Infergen ships 13 built-in adapters covering the most common stacks. Each adapter
teaches the scan engine how to recognize trackable moments in a specific framework.

---

## How adapters work

When `infergen scan` runs, it invokes every adapter whose framework appears in
`infergen.config.*`. Each adapter walks the parsed AST and file paths for the patterns
it knows — routes, forms, auth flows, API calls, error handlers — and proposes events
with confidence scores.

Adapters are additive: a Next.js project gets both the `nextjs` adapter (for routes, auth,
JSX interactions) and any other matching adapters. Confidence scores reflect how certain
each detection is:

| Score range | Meaning |
|-------------|---------|
| 0.85 – 1.0 | High certainty (e.g. explicit import of `signIn` from `next-auth/react`) |
| 0.65 – 0.84 | Good confidence (e.g. function named `handleSubmit`) |
| 0.50 – 0.64 | Lower confidence (e.g. JSX `onClick` on an anonymous handler) |

---

## Next.js — `nextjs`

**Languages:** TypeScript, JavaScript  
**Versions:** Next.js 13+ (App Router and Pages Router)  
**Activation:** detected when `"next"` is in `package.json` dependencies

**What it detects:**

- **Page views** — `pages/*.tsx` routes and `app/**/page.tsx` files → `page_viewed` with `page_path` property
- **API calls** — `pages/api/*.ts` and `app/**/route.ts` (GET/POST/PUT/DELETE/PATCH) → `api_call_*` events
- **Auth events** — `import { signIn, signOut } from 'next-auth/react'` → `user_login_*`, `user_logout_*`, `user_signup_*`
- **Form submits** — `handleSubmit`, `onSubmit`, `submitForm` function names → `form_submitted`
- **Button/interaction clicks** — JSX `<button onClick>`, `<a onClick>` → `button_clicked` / `link_clicked`

**Example events:**

```yaml
- name: page_viewed
  properties: [page_path, referrer]

- name: user_signup_completed
  properties: [method, email (pii)]

- name: button_clicked
  properties: [label]
```

**Notes:** Feature prefix derivation skips noise directories (`components/`, `hooks/`, `utils/`).
Route groups (e.g. `(marketing)/`) and dynamic segments (e.g. `[slug]`) are normalized.

---

## React Router — `react_router`

**Languages:** TypeScript, JavaScript  
**Versions:** React Router v6+  
**Activation:** detected when `"react-router-dom"` is in dependencies

**What it detects:**

- Route definitions (`<Route path="...">`, `createBrowserRouter`) → `page_viewed` with `route_path`
- Form submissions (`<Form>` component, `useSubmit` hook) → `form_submitted`
- Loader/action functions → `api_call_*`

**Example events:**

```yaml
- name: page_viewed
  properties: [route_path]

- name: form_submitted
  properties: [form_id]
```

---

## Express — `express`

**Languages:** TypeScript, JavaScript  
**Versions:** Express 4.x  
**Activation:** detected when `"express"` is in dependencies

**What it detects:**

- Route handlers (`app.get(...)`, `app.post(...)`, `router.use(...)`) → `api_request_received` / `route_handled`
- Error handlers (`app.use((err, req, res, next) => {...})`) → `error_occurred`
- Auth middleware patterns → `user_login_attempted`

**Example events:**

```yaml
- name: api_request_received
  properties: [method, path]

- name: error_occurred
  properties: [status_code, message]
```

---

## NestJS — `nestjs`

**Languages:** TypeScript  
**Versions:** NestJS 10+  
**Activation:** detected when `"@nestjs/core"` is in dependencies

**What it detects:**

- `@Controller` + `@Get`/`@Post`/`@Put`/`@Delete` decorators → `http_request_handled`
- `@Injectable` service methods → `service_called`
- Exception filters → `exception_thrown`
- Guards and interceptors → auth-related events

**Example events:**

```yaml
- name: http_request_handled
  properties: [controller, method]

- name: user_created
  properties: [role]
```

---

## Vue / Nuxt — `vue`

**Languages:** TypeScript, JavaScript  
**Versions:** Vue 3, Nuxt 3  
**Activation:** detected when `"vue"` or `"nuxt"` is in dependencies

**What it detects:**

- `definePageMeta` / router `<RouterView>` / `pages/` directory → `page_viewed`
- `<form @submit>`, `defineEmits('submit')` → `form_submitted`
- `<button @click>`, `defineEmits('click')` → `component_clicked`
- `useFetch`, `$fetch`, `useAsyncData` → `api_call_*`

**Example events:**

```yaml
- name: page_viewed
  properties: [route_name]

- name: form_submitted
  properties: [form_id]
```

---

## SvelteKit — `sveltekit`

**Languages:** TypeScript, JavaScript  
**Versions:** SvelteKit 2+  
**Activation:** detected when `"@sveltejs/kit"` or `"svelte"` is in dependencies

**What it detects:**

- `+page.svelte` / `+layout.svelte` files → `page_viewed`
- `export const actions` in `+page.server.ts` → `form_action_invoked`
- `+page.ts` / `+layout.ts` `load` functions → `page_loaded`
- `+error.svelte` → `load_error_occurred`

**Example events:**

```yaml
- name: page_viewed
  properties: [page_path]

- name: form_action_invoked
  properties: [action_name]
```

---

## Django — `django`

**Languages:** Python  
**Versions:** Django 4.x  
**Activation:** detected when `"django"` appears in `requirements.txt`, `pyproject.toml`, or `setup.py`

**What it detects:**

- URL patterns (`path(...)`, `re_path(...)` in `urls.py`) → `view_requested`
- `LoginView`, `LogoutView`, `authenticate()` calls → `user_login_attempted` / `user_logout`
- `FormView`, `CreateView`, `form.save()` → `form_submitted`
- `HttpResponseServerError`, exception views → `error_occurred`
- Django signals (`post_save`, `pre_delete`) → `model_created` / `model_deleted`

**Example events:**

```yaml
- name: view_requested
  properties: [view_name, method]

- name: user_login_attempted
  properties: [success]
```

---

## FastAPI — `fastapi`

**Languages:** Python  
**Versions:** FastAPI 0.100+  
**Activation:** detected when `"fastapi"` appears in requirements/pyproject

**What it detects:**

- `@app.get`, `@app.post`, `@router.get`, etc. → `endpoint_called`
- `OAuth2PasswordBearer`, `Depends(get_current_user)` → auth events
- `HTTPException` raises → `http_error_raised`
- Pydantic model instantiation in handlers → `user_registered` / model-specific events

**Example events:**

```yaml
- name: endpoint_called
  properties: [path, method]

- name: validation_error_raised
  properties: [field]
```

---

## Flask — `flask`

**Languages:** Python  
**Versions:** Flask 3.x  
**Activation:** detected when `"flask"` appears in requirements/pyproject

**What it detects:**

- `@app.route`, `@bp.route` decorators → `route_accessed`
- `login_user()` / `logout_user()` from Flask-Login → `user_logged_in` / `user_logged_out`
- `@app.errorhandler(404)` etc. → `error_handled`
- `WTForms` `form.validate_on_submit()` → `form_submitted`

**Example events:**

```yaml
- name: route_accessed
  properties: [endpoint, method]

- name: user_logged_in
  properties: [user_id]
```

---

## Gin — `gin`

**Languages:** Go  
**Versions:** Gin 1.9+  
**Activation:** detected when `github.com/gin-gonic/gin` is in `go.mod`

**What it detects:**

- `router.GET`, `router.POST`, `r.Group(...)` → `http_request_handled`
- `c.JSON`, `c.HTML` response calls → response events
- `c.AbortWithStatus` / `c.Error` → `error_returned`
- JWT/auth middleware patterns → `user_authenticated`

**Example events:**

```yaml
- name: http_request_handled
  properties: [method, path]

- name: user_authenticated
  properties: [user_id]
```

---

## Echo — `echo`

**Languages:** Go  
**Versions:** Echo 4.x  
**Activation:** detected when `github.com/labstack/echo` is in `go.mod`

**What it detects:**

- `e.GET`, `e.POST`, `g.Group(...)` → `http_request_handled`
- `echo.NewHTTPError` / `echo.HTTPErrorHandler` → `http_error_returned`
- JWT/BasicAuth middleware → `user_authenticated`

**Example events:**

```yaml
- name: http_request_handled
  properties: [method, path]

- name: http_error_returned
  properties: [status_code]
```

---

## net/http — `nethttp`

**Languages:** Go  
**Versions:** Go standard library  
**Activation:** detected when `go.mod` exists but no named framework is found

**What it detects:**

- `http.HandleFunc`, `http.Handle`, `mux.HandleFunc` → `http_request_handled`
- `http.Error`, `http.NotFound` → `http_error_returned`
- Custom `ServeHTTP` implementations → handler events

**Example events:**

```yaml
- name: http_request_handled
  properties: [method, pattern]
```

---

## Rails — `rails`

**Languages:** Ruby  
**Versions:** Rails 7+ (Devise, Turbo support)  
**Activation:** detected when `Gemfile` includes `"rails"`

**What it detects:**

- Routes in `config/routes.rb` (`resources :users`, `get "/path"`) → `controller_action_called`
- Devise callbacks (`after_sign_in_path_for`, `before_action :authenticate_user!`) → `user_signed_in` / `user_signed_out`
- ActiveRecord callbacks (`after_create`, `before_destroy`) → `model_created` / `model_deleted`
- Turbo Stream broadcasts → `turbo_stream_broadcast`

**Example events:**

```yaml
- name: controller_action_called
  properties: [controller, action]

- name: user_signed_in
  properties: [user_id]
```

---

## Adding a custom adapter

See the [Plugin SDK](../plugin-sdk.md) documentation to scaffold and implement a custom
framework adapter using the `Adapter` trait.

```bash
infergen plugin scaffold adapter my-adapter --framework myframework
infergen plugin list-types
```
