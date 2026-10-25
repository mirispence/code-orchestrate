//! Command errors reach the UI as `{ kind, message, ...fields }` with camelCase
//! keys (`src/lib/api.ts` types them).
//!
//! An error enum derives the `{ kind, ...fields }` part and `ui_error!` adds
//! `message`, the enum's `Display` text:
//!
//! ```ignore
//! #[derive(Debug, thiserror::Error, serde::Serialize)]
//! #[serde(remote = "Self", tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
//! pub enum MyError { … }
//! ui_error!(MyError);
//! ```
//!
//! `remote = "Self"` makes the derive an inherent `MyError::serialize` the
//! macro calls, rather than the `Serialize` impl itself. A tuple field that is
//! only there for the message is marked `#[serde(serialize_with = "crate::ui_error::omit")]`.

use serde::{Serialize, Serializer};

/// Serialise the derived `{ kind, ...fields }` with `message` added.
pub fn with_message<S: Serializer>(
    fields: Result<serde_json::Value, serde_json::Error>,
    message: String,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let mut value = fields.map_err(serde::ser::Error::custom)?;
    let Some(object) = value.as_object_mut() else {
        return Err(serde::ser::Error::custom("an error must serialise as an object"));
    };
    object.insert("message".into(), message.into());
    value.serialize(serializer)
}

/// Leaves a field out of the JSON (it is in `message` already).
pub fn omit<T, S: Serializer>(_: &T, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_unit()
}

/// `impl Serialize` for an error enum: its derived JSON plus `message`.
macro_rules! ui_error {
    ($error:ty) => {
        impl serde::Serialize for $error {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                // The inherent `serialize` from `#[serde(remote = "Self")]`.
                let fields = <$error>::serialize(self, serde_json::value::Serializer);
                $crate::ui_error::with_message(fields, self.to_string(), serializer)
            }
        }
    };
}
pub(crate) use ui_error;
