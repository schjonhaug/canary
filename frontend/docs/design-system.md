# Frontend Design System Patterns

## Buttons

- Use the default `Button` variant for primary actions, including form submits and save actions.
- Use `variant="outline"` for secondary actions that sit beside or below a primary action, including cancel, back, and alternate navigation actions.
- Use `variant="destructive"` only for destructive confirmation actions, such as delete buttons.
- Use `variant="ghost"` for low-emphasis chrome actions, such as icon-only edit controls, row actions, and header menus.
- Use `size="sm"` for compact controls inside dense lists or inline editors. Leave form submits at the default size unless the whole form uses a compact layout.

## Errors And Success

- Use `ErrorDisplay` with `variant="inline"` for form-level and modal-level failures.
- Inline `ErrorDisplay` omits the default title unless a title is supplied explicitly.
- Use `FieldError` for field-level validation messages directly under an input.
- Use `FieldError announce` only for dynamic errors that should interruptively announce after interaction.
- Use `SuccessDisplay` for short form or verification success messages.
- Use `SuccessDisplay` with `variant="compact"` for field-level verification success that should stay inline.
- `ErrorDisplay` with `variant="card"` is for page-level async failures and uses alert semantics.
- Keep page-level status, connection, or warning banners on `Alert` when they are not user-correctable form errors.
