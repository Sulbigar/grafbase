use super::{DynamicParse, SDLDefinitionScalar};
use crate::{Error, InputValueError, InputValueResult};
use chrono::{DateTime, Utc};
use dynaql_value::ConstValue;

pub struct DatetimeScalar;

impl<'a> SDLDefinitionScalar<'a> for DatetimeScalar {
    fn name() -> Option<&'a str> {
        Some("Datetime")
    }

    fn specified_by() -> Option<&'a str> {
        Some("https://datatracker.ietf.org/doc/html/rfc3339")
    }

    fn description() -> Option<&'a str> {
        Some(
            r#"
            A date-time string at UTC, such as 2007-12-03T10:15:30Z, is compliant with the date-time format outlined in section 5.6 of the RFC 3339
            profile of the ISO 8601 standard for representation of dates and times using the Gregorian calendar.

            This scalar is a description of an exact instant on the timeline such as the instant that a user account was created.

            # Input Coercion

            When expected as an input type, only RFC 3339 compliant date-time strings are accepted. All other input values raise a query error indicating an incorrect type.

            # Result Coercion

            Where an RFC 3339 compliant date-time string has a time-zone other than UTC, it is shifted to UTC.
            For example, the date-time string 2016-01-01T14:10:20+01:00 is shifted to 2016-01-01T13:10:20Z.
            "#,
        )
    }
}

impl DynamicParse for DatetimeScalar {
    fn is_valid(value: &ConstValue) -> bool {
        match value {
            ConstValue::String(val) => val.parse::<DateTime<Utc>>().is_ok(),
            _ => false,
        }
    }

    fn to_value(value: serde_json::Value) -> Result<ConstValue, Error> {
        match value {
            serde_json::Value::String(v) => Ok(ConstValue::String(v)),
            _ => Err(Error::new(
                "Data violation: Cannot coerce the initial value to a Datetime",
            )),
        }
    }

    fn parse(value: ConstValue) -> InputValueResult<serde_json::Value> {
        match value {
            ConstValue::String(val) => {
                let date = val
                    .parse::<DateTime<Utc>>()
                    .map_err(|err| InputValueError::ty_custom("DateTime", err))?;

                Ok(serde_json::Value::String(date.to_rfc3339()))
            }
            _ => Err(InputValueError::ty_custom(
                "Datetime",
                "Cannot parse into a String",
            )),
        }
    }
}
