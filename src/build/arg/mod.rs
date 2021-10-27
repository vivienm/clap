#[cfg(debug_assertions)]
pub mod debug_asserts;
mod possible_value;
mod settings;
#[cfg(test)]
mod tests;
mod value_hint;

pub use self::possible_value::PossibleValue;
pub use self::settings::{ArgFlags, ArgSettings};
pub use self::value_hint::ValueHint;

// Std
use std::{
    borrow::Cow,
    cmp::{Ord, Ordering},
    error::Error,
    ffi::OsStr,
    fmt::{self, Display, Formatter},
    iter, str,
    sync::{Arc, Mutex},
};
#[cfg(feature = "env")]
use std::{env, ffi::OsString};

// Third Party
#[cfg(feature = "regex")]
use ::regex::Regex;

#[cfg(feature = "yaml")]
use yaml_rust::Yaml;

// Internal
use crate::{
    build::usage_parser::UsageParser,
    util::{Id, Key},
    INTERNAL_ERROR_MSG,
};

#[cfg(feature = "regex")]
mod regex;

#[cfg(feature = "regex")]
pub use self::regex::RegexRef;

type Validator<'a> = dyn FnMut(&str) -> Result<(), Box<dyn Error + Send + Sync>> + Send + 'a;
type ValidatorOs<'a> = dyn FnMut(&OsStr) -> Result<(), Box<dyn Error + Send + Sync>> + Send + 'a;

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) enum ArgProvider {
    Generated,
    GeneratedMutated,
    User,
}

impl Default for ArgProvider {
    fn default() -> Self {
        ArgProvider::User
    }
}

/// The abstract representation of a command line argument. Used to set all the options and
/// relationships that define a valid argument for the program.
///
/// There are two methods for constructing [`Arg`]s, using the builder pattern and setting options
/// manually, or using a usage string which is far less verbose but has fewer options. You can also
/// use a combination of the two methods to achieve the best of both worlds.
///
/// # Examples
///
/// ```rust
/// # use clap::Arg;
/// // Using the traditional builder pattern and setting each option manually
/// let cfg = Arg::new("config")
///       .short('c')
///       .long("config")
///       .takes_value(true)
///       .value_name("FILE")
///       .about("Provides a config file to myprog");
/// // Using a usage string (setting a similar argument to the one above)
/// let input = Arg::from("-i, --input=[FILE] 'Provides an input file to the program'");
/// ```
#[allow(missing_debug_implementations)]
#[derive(Default, Clone)]
pub struct Arg<'help> {
    pub(crate) id: Id,
    pub(crate) provider: ArgProvider,
    pub(crate) name: &'help str,
    pub(crate) about: Option<&'help str>,
    pub(crate) long_about: Option<&'help str>,
    pub(crate) blacklist: Vec<Id>,
    pub(crate) settings: ArgFlags,
    pub(crate) overrides: Vec<Id>,
    pub(crate) groups: Vec<Id>,
    pub(crate) requires: Vec<(Option<&'help str>, Id)>,
    pub(crate) r_ifs: Vec<(Id, &'help str)>,
    pub(crate) r_ifs_all: Vec<(Id, &'help str)>,
    pub(crate) r_unless: Vec<Id>,
    pub(crate) short: Option<char>,
    pub(crate) long: Option<&'help str>,
    pub(crate) aliases: Vec<(&'help str, bool)>, // (name, visible)
    pub(crate) short_aliases: Vec<(char, bool)>, // (name, visible)
    pub(crate) disp_ord: usize,
    pub(crate) unified_ord: usize,
    pub(crate) possible_vals: Vec<PossibleValue<'help>>,
    pub(crate) val_names: Vec<&'help str>,
    pub(crate) num_vals: Option<usize>,
    pub(crate) max_occurs: Option<usize>,
    pub(crate) max_vals: Option<usize>,
    pub(crate) min_vals: Option<usize>,
    pub(crate) validator: Option<Arc<Mutex<Validator<'help>>>>,
    pub(crate) validator_os: Option<Arc<Mutex<ValidatorOs<'help>>>>,
    pub(crate) val_delim: Option<char>,
    pub(crate) default_vals: Vec<&'help OsStr>,
    pub(crate) default_vals_ifs: Vec<(Id, Option<&'help OsStr>, Option<&'help OsStr>)>,
    pub(crate) default_missing_vals: Vec<&'help OsStr>,
    #[cfg(feature = "env")]
    pub(crate) env: Option<(&'help OsStr, Option<OsString>)>,
    pub(crate) terminator: Option<&'help str>,
    pub(crate) index: Option<usize>,
    pub(crate) help_heading: Option<Option<&'help str>>,
    pub(crate) global: bool,
    pub(crate) exclusive: bool,
    pub(crate) value_hint: ValueHint,
}

/// Getters
impl<'help> Arg<'help> {
    /// Get the name of the argument
    #[inline]
    pub fn get_name(&self) -> &str {
        self.name
    }

    /// Get the help specified for this argument, if any
    #[inline]
    pub fn get_about(&self) -> Option<&str> {
        self.about
    }

    /// Get the long help specified for this argument, if any
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use clap::Arg;
    /// let arg = Arg::new("foo").long_about("long about");
    /// assert_eq!(Some("long about"), arg.get_long_about());
    /// ```
    ///
    #[inline]
    pub fn get_long_about(&self) -> Option<&str> {
        self.long_about
    }

    /// Get the help heading specified for this argument, if any
    #[inline]
    pub fn get_help_heading(&self) -> Option<&str> {
        self.help_heading.unwrap_or_default()
    }

    /// Get the short option name for this argument, if any
    #[inline]
    pub fn get_short(&self) -> Option<char> {
        self.short
    }

    /// Get visible short aliases for this argument, if any
    #[inline]
    pub fn get_visible_short_aliases(&self) -> Option<Vec<char>> {
        if self.short_aliases.is_empty() {
            None
        } else {
            Some(
                self.short_aliases
                    .iter()
                    .filter_map(|(c, v)| if *v { Some(c) } else { None })
                    .copied()
                    .collect(),
            )
        }
    }

    /// Get the short option name and its visible aliases, if any
    #[inline]
    pub fn get_short_and_visible_aliases(&self) -> Option<Vec<char>> {
        let mut shorts = match self.short {
            Some(short) => vec![short],
            None => return None,
        };
        if let Some(aliases) = self.get_visible_short_aliases() {
            shorts.extend(aliases);
        }
        Some(shorts)
    }

    /// Get the long option name for this argument, if any
    #[inline]
    pub fn get_long(&self) -> Option<&str> {
        self.long
    }

    /// Get visible aliases for this argument, if any
    #[inline]
    pub fn get_visible_aliases(&self) -> Option<Vec<&str>> {
        if self.aliases.is_empty() {
            None
        } else {
            Some(
                self.aliases
                    .iter()
                    .filter_map(|(s, v)| if *v { Some(s) } else { None })
                    .copied()
                    .collect(),
            )
        }
    }

    /// Get the long option name and its visible aliases, if any
    #[inline]
    pub fn get_long_and_visible_aliases(&self) -> Option<Vec<&str>> {
        let mut longs = match self.long {
            Some(long) => vec![long],
            None => return None,
        };
        if let Some(aliases) = self.get_visible_aliases() {
            longs.extend(aliases);
        }
        Some(longs)
    }

    /// Get the list of the possible values for this argument, if any
    #[inline]
    pub fn get_possible_values(&self) -> Option<&[PossibleValue]> {
        if self.possible_vals.is_empty() {
            None
        } else {
            Some(&self.possible_vals)
        }
    }

    /// Get the names of values for this argument.
    #[inline]
    pub fn get_value_names(&self) -> Option<&[&str]> {
        if self.val_names.is_empty() {
            None
        } else {
            Some(&self.val_names)
        }
    }

    /// Get the number of values for this argument.
    #[inline]
    pub fn get_num_vals(&self) -> Option<usize> {
        self.num_vals
    }

    /// Get the index of this argument, if any
    #[inline]
    pub fn get_index(&self) -> Option<usize> {
        self.index
    }

    /// Get the value hint of this argument
    pub fn get_value_hint(&self) -> ValueHint {
        self.value_hint
    }

    /// Get information on if this argument is global or not
    pub fn get_global(&self) -> bool {
        self.global
    }

    /// Get the environment variable name specified for this argument, if any
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use std::ffi::OsStr;
    /// # use clap::Arg;
    /// let arg = Arg::new("foo").env("ENVIRONMENT");
    /// assert_eq!(Some(OsStr::new("ENVIRONMENT")), arg.get_env());
    /// ```
    #[cfg(feature = "env")]
    pub fn get_env(&self) -> Option<&OsStr> {
        self.env.as_ref().map(|x| x.0)
    }

    /// Get the default values specified for this argument, if any
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use clap::Arg;
    /// let arg = Arg::new("foo").default_value("default value");
    /// assert_eq!(&["default value"], arg.get_default_values());
    /// ```
    pub fn get_default_values(&self) -> &[&OsStr] {
        &self.default_vals
    }

    /// Checks whether this argument is a positional or not.
    ///
    /// # Examples
    ///
    /// ```
    /// # use clap::Arg;
    /// let arg = Arg::new("foo");
    /// assert_eq!(true, arg.is_positional());
    ///
    /// let arg = Arg::new("foo").long("foo");
    /// assert_eq!(false, arg.is_positional());
    /// ```
    pub fn is_positional(&self) -> bool {
        self.long.is_none() && self.short.is_none()
    }
}

impl<'help> Arg<'help> {
    /// Creates a new instance of [`Arg`] using a unique string name. The name will be used to get
    /// information about whether or not the argument was used at runtime, get values, set
    /// relationships with other args, etc..
    ///
    /// **NOTE:** In the case of arguments that take values (i.e. [`Arg::takes_value(true)`])
    /// and positional arguments (i.e. those without a preceding `-` or `--`) the name will also
    /// be displayed when the user prints the usage/help information of the program.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use clap::{App, Arg};
    /// Arg::new("config")
    /// # ;
    /// ```
    /// [`Arg::takes_value(true)`]: Arg::takes_value()
    pub fn new<S: Into<&'help str>>(n: S) -> Self {
        let name = n.into();
        Arg {
            id: Id::from(&*name),
            name,
            disp_ord: 999,
            unified_ord: 999,
            ..Default::default()
        }
    }

    /// Deprecated, see [`Arg::new`]
    #[deprecated(since = "3.0.0", note = "Replaced with `Arg::new`")]
    pub fn with_name<S: Into<&'help str>>(n: S) -> Self {
        Self::new(n)
    }

    /// Deprecated, see [`Arg::from`]
    #[cfg(feature = "yaml")]
    #[deprecated(since = "3.0.0", note = "Replaced with `Arg::from`")]
    pub fn from_yaml(y: &'help Yaml) -> Self {
        Self::from(y)
    }

    /// Deprecated, see [`Arg::from`]
    #[deprecated(since = "3.0.0", note = "Replaced with `Arg::from`")]
    pub fn from_usage(u: &'help str) -> Self {
        Self::from(u)
    }

    pub(crate) fn generated(mut self) -> Self {
        self.provider = ArgProvider::Generated;
        self
    }

    /// Sets the short version of the argument without the preceding `-`.
    ///
    /// By default `clap` automatically assigns `V` and `h` to the auto-generated `version` and
    /// `help` arguments respectively. You may use the uppercase `V` or lowercase `h` for your own
    /// arguments, in which case `clap` simply will not assign those to the auto-generated
    /// `version` or `help` arguments.
    ///
    /// # Examples
    ///
    /// To set [`short`] use a single valid UTF-8 character. If you supply a leading `-` such as
    /// `-c`, the `-` will be stripped.
    ///
    /// ```rust
    /// # use clap::{App, Arg};
    /// Arg::new("config")
    ///     .short('c')
    /// # ;
    /// ```
    ///
    /// Setting [`short`] allows using the argument via a single hyphen (`-`) such as `-c`
    ///
    /// ```rust
    /// # use clap::{App, Arg};
    /// let m = App::new("prog")
    ///     .arg(Arg::new("config")
    ///         .short('c'))
    ///     .get_matches_from(vec![
    ///         "prog", "-c"
    ///     ]);
    ///
    /// assert!(m.is_present("config"));
    /// ```
    /// [`short`]: Arg::short()
    #[inline]
    pub fn short(mut self, s: char) -> Self {
        assert!(s != '-', "short option name cannot be `-`");

        self.short = Some(s);
        self
    }

    /// Sets the long version of the argument without the preceding `--`.
    ///
    /// By default `clap` automatically assigns `version` and `help` to the auto-generated
    /// `version` and `help` arguments respectively. You may use the word `version` or `help` for
    /// the long form of your own arguments, in which case `clap` simply will not assign those to
    /// the auto-generated `version` or `help` arguments.
    ///
    /// **NOTE:** Any leading `-` characters will be stripped
    ///
    /// # Examples
    ///
    /// To set `long` use a word containing valid UTF-8 codepoints. If you supply a double leading
    /// `--` such as `--config` they will be stripped. Hyphens in the middle of the word, however,
    /// will *not* be stripped (i.e. `config-file` is allowed)
    ///
    /// ```rust
    /// # use clap::{App, Arg};
    /// Arg::new("cfg")
    ///     .long("config")
    /// # ;
    /// ```
    ///
    /// Setting `long` allows using the argument via a double hyphen (`--`) such as `--config`
    ///
    /// ```rust
    /// # use clap::{App, Arg};
    /// let m = App::new("prog")
    ///     .arg(Arg::new("cfg")
    ///         .long("config"))
    ///     .get_matches_from(vec![
    ///         "prog", "--config"
    ///     ]);
    ///
    /// assert!(m.is_present("cfg"));
    /// ```
    #[inline]
    pub fn long(mut self, l: &'help str) -> Self {
        self.long = Some(l.trim_start_matches(|c| c == '-'));
        self
    }

    /// Allows adding a [`Arg`] alias, which function as "hidden" arguments that
    /// automatically dispatch as if this argument was used. This is more efficient, and easier
    /// than creating multiple hidden arguments as one only needs to check for the existence of
    /// this command, and not all variants.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use clap::{App, Arg};
    /// let m = App::new("prog")
    ///             .arg(Arg::new("test")
    ///             .long("test")
    ///             .alias("alias")
    ///             .takes_value(true))
    ///        .get_matches_from(vec![
    ///             "prog", "--alias", "cool"
    ///         ]);
    /// assert!(m.is_present("test"));
    /// assert_eq!(m.value_of("test"), Some("cool"));
    /// ```
    pub fn alias<S: Into<&'help str>>(mut self, name: S) -> Self {
        self.aliases.push((name.into(), false));
        self
    }

    /// Allows adding a [`Arg`] alias, which function as "hidden" arguments that
    /// automatically dispatch as if this argument was used. This is more efficient, and easier
    /// than creating multiple hidden arguments as one only needs to check for the existence of
    /// this command, and not all variants.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use clap::{App, Arg};
    /// let m = App::new("prog")
    ///             .arg(Arg::new("test")
    ///             .short('t')
    ///             .short_alias('e')
    ///             .takes_value(true))
    ///        .get_matches_from(vec![
    ///             "prog", "-e", "cool"
    ///         ]);
    /// assert!(m.is_present("test"));
    /// assert_eq!(m.value_of("test"), Some("cool"));
    /// ```
    pub fn short_alias(mut self, name: char) -> Self {
        assert!(name != '-', "short alias name cannot be `-`");

        self.short_aliases.push((name, false));
        self
    }

    /// Allows adding [`Arg`] aliases, which function as "hidden" arguments that
    /// automatically dispatch as if this argument was used. This is more efficient, and easier
    /// than creating multiple hidden subcommands as one only needs to check for the existence of
    /// this command, and not all variants.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use clap::{App, Arg};
    /// let m = App::new("prog")
    ///             .arg(Arg::new("test")
    ///                     .long("test")
    ///                     .aliases(&["do-stuff", "do-tests", "tests"])
    ///                     .about("the file to add")
    ///                     .required(false))
    ///             .get_matches_from(vec![
    ///                 "prog", "--do-tests"
    ///             ]);
    /// assert!(m.is_present("test"));
    /// ```
    pub fn aliases(mut self, names: &[&'help str]) -> Self {
        self.aliases.extend(names.iter().map(|&x| (x, false)));
        self
    }

    /// Allows adding [`Arg`] aliases, which function as "hidden" arguments that
    /// automatically dispatch as if this argument was used. This is more efficient, and easier
    /// than creating multiple hidden subcommands as one only needs to check for the existence of
    /// this command, and not all variants.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use clap::{App, Arg};
    /// let m = App::new("prog")
    ///             .arg(Arg::new("test")
    ///                     .short('t')
    ///                     .short_aliases(&['e', 's'])
    ///                     .about("the file to add")
    ///                     .required(false))
    ///             .get_matches_from(vec![
    ///                 "prog", "-s"
    ///             ]);
    /// assert!(m.is_present("test"));
    /// ```
    pub fn short_aliases(mut self, names: &[char]) -> Self {
        for s in names {
            assert!(s != &'-', "short alias name cannot be `-`");
            self.short_aliases.push((*s, false));
        }
        self
    }

    /// Allows adding a [`Arg`] alias that functions exactly like those defined with
    /// [`Arg::alias`], except that they are visible inside the help message.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use clap::{App, Arg};
    /// let m = App::new("prog")
    ///             .arg(Arg::new("test")
    ///                 .visible_alias("something-awesome")
    ///                 .long("test")
    ///                 .takes_value(true))
    ///        .get_matches_from(vec![
    ///             "prog", "--something-awesome", "coffee"
    ///         ]);
    /// assert!(m.is_present("test"));
    /// assert_eq!(m.value_of("test"), Some("coffee"));
    /// ```
    /// [`App::alias`]: Arg::alias()
    pub fn visible_alias<S: Into<&'help str>>(mut self, name: S) -> Self {
        self.aliases.push((name.into(), true));
        self
    }

    /// Allows adding a [`Arg`] alias that functions exactly like those defined with
    /// [`Arg::alias`], except that they are visible inside the help message.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use clap::{App, Arg};
    /// let m = App::new("prog")
    ///             .arg(Arg::new("test")
    ///                 .long("test")
    ///                 .visible_short_alias('t')
    ///                 .takes_value(true))
    ///        .get_matches_from(vec![
    ///             "prog", "-t", "coffee"
    ///         ]);
    /// assert!(m.is_present("test"));
    /// assert_eq!(m.value_of("test"), Some("coffee"));
    /// ```
    /// [`App::alias`]: Arg::short_alias()
    pub fn visible_short_alias(mut self, name: char) -> Self {
        assert!(name != '-', "short alias name cannot be `-`");

        self.short_aliases.push((name, true));
        self
    }

    /// Allows adding multiple [`Arg`] aliases that functions exactly like those defined
    /// with [`Arg::aliases`], except that they are visible inside the help message.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use clap::{App, Arg};
    /// let m = App::new("prog")
    ///             .arg(Arg::new("test")
    ///                 .long("test")
    ///                 .visible_aliases(&["something", "awesome", "cool"]))
    ///        .get_matches_from(vec![
    ///             "prog", "--awesome"
    ///         ]);
    /// assert!(m.is_present("test"));
    /// ```
    /// [`App::aliases`]: Arg::aliases()
    pub fn visible_aliases(mut self, names: &[&'help str]) -> Self {
        self.aliases.extend(names.iter().map(|n| (*n, true)));
        self
    }

    /// Allows adding multiple [`Arg`] aliases that functions exactly like those defined
    /// with [`Arg::aliases`], except that they are visible inside the help message.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use clap::{App, Arg};
    /// let m = App::new("prog")
    ///             .arg(Arg::new("test")
    ///                 .long("test")
    ///                 .visible_short_aliases(&['t', 'e']))
    ///        .get_matches_from(vec![
    ///             "prog", "-t"
    ///         ]);
    /// assert!(m.is_present("test"));
    /// ```
    /// [`App::aliases`]: Arg::short_aliases()
    pub fn visible_short_aliases(mut self, names: &[char]) -> Self {
        for n in names {
            assert!(n != &'-', "short alias name cannot be `-`");
            self.short_aliases.push((*n, true));
        }
        self
    }

    /// Sets the short help text of the argument that will be displayed to the user when they print
    /// the help information with `-h`. Typically, this is a short (one line) description of the
    /// arg.
    ///
    /// **NOTE:** If only `Arg::about` is provided, and not [`Arg::long_about`] but the user requests
    /// `--help` clap will still display the contents of `help` appropriately
    ///
    /// **NOTE:** Only `Arg::about` is used in completion script generation in order to be concise
    ///
    /// # Examples
    ///
    /// Any valid UTF-8 is allowed in the help text. The one exception is when one wishes to
    /// include a newline in the help text and have the following text be properly aligned with all
    /// the other help text.
    ///
    /// ```rust
    /// # use clap::{App, Arg};
    /// Arg::new("config")
    ///     .about("The config file used by the myprog")
    /// # ;
    /// ```
    ///
    /// Setting `about` displays a short message to the side of the argument when the user passes
    /// `-h` or `--help` (by default).
    ///
    /// ```rust
    /// # use clap::{App, Arg};
    /// let m = App::new("prog")
    ///     .arg(Arg::new("cfg")
    ///         .long("config")
    ///         .about("Some help text describing the --config arg"))
    ///     .get_matches_from(vec![
    ///         "prog", "--help"
    ///     ]);
    /// ```
    ///
    /// The above example displays
    ///
    /// ```notrust
    /// helptest
    ///
    /// USAGE:
    ///    helptest [OPTIONS]
    ///
    /// OPTIONS:
    ///     --config     Some help text describing the --config arg
    /// -h, --help       Print help information
    /// -V, --version    Print version information
    /// ```
    /// [`Arg::long_about`]: Arg::long_about()
    #[inline]
    pub fn about(mut self, h: &'help str) -> Self {
        self.about = Some(h);
        self
    }

    /// Deprecated, see [`Arg::about`]
    #[deprecated(since = "3.0.0", note = "Replaced with `Arg::about`")]
    pub fn help(self, h: &'help str) -> Self {
        self.about(h)
    }

    /// Sets the long help text of the argument that will be displayed to the user when they print
    /// the help information with `--help`. Typically this a more detailed (multi-line) message
    /// that describes the arg.
    ///
    /// **NOTE:** If only `long_about` is provided, and not [`Arg::about`] but the user requests `-h`
    /// clap will still display the contents of `long_about` appropriately
    ///
    /// **NOTE:** Only [`Arg::about`] is used in completion script generation in order to be concise
    ///
    /// # Examples
    ///
    /// Any valid UTF-8 is allowed in the help text. The one exception is when one wishes to
    /// include a newline in the help text and have the following text be properly aligned with all
    /// the other help text.
    ///
    /// ```rust
    /// # use clap::{App, Arg};
    /// Arg::new("config")
    ///     .long_about(
    /// "The config file used by the myprog must be in JSON format
    /// with only valid keys and may not contain other nonsense
    /// that cannot be read by this program. Obviously I'm going on
    /// and on, so I'll stop now.")
    /// # ;
    /// ```
    ///
    /// Setting `help` displays a short message to the side of the argument when the user passes
    /// `-h` or `--help` (by default).
    ///
    /// ```rust
    /// # use clap::{App, Arg};
    /// let m = App::new("prog")
    ///     .arg(Arg::new("cfg")
    ///         .long("config")
    ///         .long_about(
    /// "The config file used by the myprog must be in JSON format
    /// with only valid keys and may not contain other nonsense
    /// that cannot be read by this program. Obviously I'm going on
    /// and on, so I'll stop now."))
    ///     .get_matches_from(vec![
    ///         "prog", "--help"
    ///     ]);
    /// ```
    ///
    /// The above example displays
    ///
    /// ```text
    /// prog
    ///
    /// USAGE:
    ///     prog [OPTIONS]
    ///
    /// OPTIONS:
    ///         --config
    ///             The config file used by the myprog must be in JSON format
    ///             with only valid keys and may not contain other nonsense
    ///             that cannot be read by this program. Obviously I'm going on
    ///             and on, so I'll stop now.
    ///
    ///     -h, --help
    ///             Print help information
    ///
    ///     -V, --version
    ///             Print version information
    /// ```
    /// [`Arg::about`]: Arg::about()
    #[inline]
    pub fn long_about(mut self, h: &'help str) -> Self {
        self.long_about = Some(h);
        self
    }

    /// Deprecated, see [`Arg::long_about`]
    #[deprecated(since = "3.0.0", note = "Replaced with `Arg::long_about`")]
    pub fn long_help(self, h: &'help str) -> Self {
        self.long_about(h)
    }

    /// Set this arg as [required] as long as the specified argument is not present at runtime.
    ///
    /// **Pro Tip:** Using `Arg::required_unless_present` implies [`Arg::required`] and is therefore not
    /// mandatory to also set.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use clap::Arg;
    /// Arg::new("config")
    ///     .required_unless_present("debug")
    /// # ;
    /// ```
    ///
    /// In the following example, the required argument is *not* provided,
    /// but it's not an error because the `unless` arg has been supplied.
    ///
    /// ```rust
    /// # use clap::{App, Arg};
    /// let res = App::new("prog")
    ///     .arg(Arg::new("cfg")
    ///         .required_unless_present("dbg")
    ///         .takes_value(true)
    ///         .long("config"))
    ///     .arg(Arg::new("dbg")
    ///         .long("debug"))
    ///     .try_get_matches_from(vec![
    ///         "prog", "--debug"
    ///     ]);
    ///
    /// assert!(res.is_ok());
    /// ```
    ///
    /// Setting `Arg::required_unless_present(name)` and *not* supplying `name` or this arg is an error.
    ///
    /// ```rust
    /// # use clap::{App, Arg, ErrorKind};
    /// let res = App::new("prog")
    ///     .arg(Arg::new("cfg")
    ///         .required_unless_present("dbg")
    ///         .takes_value(true)
    ///         .long("config"))
    ///     .arg(Arg::new("dbg")
    ///         .long("debug"))
    ///     .try_get_matches_from(vec![
    ///         "prog"
    ///     ]);
    ///
    /// assert!(res.is_err());
    /// assert_eq!(res.unwrap_err().kind, ErrorKind::MissingRequiredArgument);
    /// ```
    /// [required]: Arg::required()
    pub fn required_unless_present<T: Key>(mut self, arg_id: T) -> Self {
        self.r_unless.push(arg_id.into());
        self
    }

    /// Deprecated, see [`Arg::required_unless_present`]
    #[deprecated(since = "3.0.0", note = "Replaced with `Arg::required_unless_present`")]
    pub fn required_unless<T: Key>(self, arg_id: T) -> Self {
        self.required_unless_present(arg_id)
    }

    /// Sets this arg as [required] unless *all* of the specified arguments are present at runtime.
    ///
    /// In other words, parsing will succeed only if user either
    /// * supplies the `self` arg.
    /// * supplies *all* of the `names` arguments.
    ///
    /// **NOTE:** If you wish for this argument to only be required unless *any of* these args are
    /// present see [`Arg::required_unless_present_any`]
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use clap::Arg;
    /// Arg::new("config")
    ///     .required_unless_present_all(&["cfg", "dbg"])
    /// # ;
    /// ```
    ///
    /// In the following example, the required argument is *not* provided, but it's not an error
    /// because *all* of the `names` args have been supplied.
    ///
    /// ```rust
    /// # use clap::{App, Arg};
    /// let res = App::new("prog")
    ///     .arg(Arg::new("cfg")
    ///         .required_unless_present_all(&["dbg", "infile"])
    ///         .takes_value(true)
    ///         .long("config"))
    ///     .arg(Arg::new("dbg")
    ///         .long("debug"))
    ///     .arg(Arg::new("infile")
    ///         .short('i')
    ///         .takes_value(true))
    ///     .try_get_matches_from(vec![
    ///         "prog", "--debug", "-i", "file"
    ///     ]);
    ///
    /// assert!(res.is_ok());
    /// ```
    ///
    /// Setting [`Arg::required_unless_present_all(names)`] and *not* supplying
    /// either *all* of `unless` args or the `self` arg is an error.
    ///
    /// ```rust
    /// # use clap::{App, Arg, ErrorKind};
    /// let res = App::new("prog")
    ///     .arg(Arg::new("cfg")
    ///         .required_unless_present_all(&["dbg", "infile"])
    ///         .takes_value(true)
    ///         .long("config"))
    ///     .arg(Arg::new("dbg")
    ///         .long("debug"))
    ///     .arg(Arg::new("infile")
    ///         .short('i')
    ///         .takes_value(true))
    ///     .try_get_matches_from(vec![
    ///         "prog"
    ///     ]);
    ///
    /// assert!(res.is_err());
    /// assert_eq!(res.unwrap_err().kind, ErrorKind::MissingRequiredArgument);
    /// ```
    /// [required]: Arg::required()
    /// [`Arg::required_unless_present_any`]: Arg::required_unless_present_any()
    /// [`Arg::required_unless_present_all(names)`]: Arg::required_unless_present_all()
    pub fn required_unless_present_all<T, I>(mut self, names: I) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Key,
    {
        self.r_unless.extend(names.into_iter().map(Id::from));
        self.setting(ArgSettings::RequiredUnlessAll)
    }

    /// Deprecated, see [`Arg::required_unless_present_all`]
    #[deprecated(
        since = "3.0.0",
        note = "Replaced with `Arg::required_unless_present_all`"
    )]
    pub fn required_unless_all<T, I>(self, names: I) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Key,
    {
        self.required_unless_present_all(names)
    }

    /// Sets this arg as [required] unless *any* of the specified arguments are present at runtime.
    ///
    /// In other words, parsing will succeed only if user either
    /// * supplies the `self` arg.
    /// * supplies *one or more* of the `unless` arguments.
    ///
    /// **NOTE:** If you wish for this argument to be required unless *all of* these args are
    /// present see [`Arg::required_unless_present_all`]
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use clap::Arg;
    /// Arg::new("config")
    ///     .required_unless_present_any(&["cfg", "dbg"])
    /// # ;
    /// ```
    ///
    /// Setting [`Arg::required_unless_present_any(names)`] requires that the argument be used at runtime
    /// *unless* *at least one of* the args in `names` are present. In the following example, the
    /// required argument is *not* provided, but it's not an error because one the `unless` args
    /// have been supplied.
    ///
    /// ```rust
    /// # use clap::{App, Arg};
    /// let res = App::new("prog")
    ///     .arg(Arg::new("cfg")
    ///         .required_unless_present_any(&["dbg", "infile"])
    ///         .takes_value(true)
    ///         .long("config"))
    ///     .arg(Arg::new("dbg")
    ///         .long("debug"))
    ///     .arg(Arg::new("infile")
    ///         .short('i')
    ///         .takes_value(true))
    ///     .try_get_matches_from(vec![
    ///         "prog", "--debug"
    ///     ]);
    ///
    /// assert!(res.is_ok());
    /// ```
    ///
    /// Setting [`Arg::required_unless_present_any(names)`] and *not* supplying *at least one of* `names`
    /// or this arg is an error.
    ///
    /// ```rust
    /// # use clap::{App, Arg, ErrorKind};
    /// let res = App::new("prog")
    ///     .arg(Arg::new("cfg")
    ///         .required_unless_present_any(&["dbg", "infile"])
    ///         .takes_value(true)
    ///         .long("config"))
    ///     .arg(Arg::new("dbg")
    ///         .long("debug"))
    ///     .arg(Arg::new("infile")
    ///         .short('i')
    ///         .takes_value(true))
    ///     .try_get_matches_from(vec![
    ///         "prog"
    ///     ]);
    ///
    /// assert!(res.is_err());
    /// assert_eq!(res.unwrap_err().kind, ErrorKind::MissingRequiredArgument);
    /// ```
    /// [required]: Arg::required()
    /// [`Arg::required_unless_present_any(names)`]: Arg::required_unless_present_any()
    /// [`Arg::required_unless_present_all`]: Arg::required_unless_present_all()
    pub fn required_unless_present_any<T, I>(mut self, names: I) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Key,
    {
        self.r_unless.extend(names.into_iter().map(Id::from));
        self
    }

    /// Deprecated, see [`Arg::required_unless_present_any`]
    #[deprecated(
        since = "3.0.0",
        note = "Replaced with `Arg::required_unless_present_any`"
    )]
    pub fn required_unless_any<T, I>(self, names: I) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Key,
    {
        self.required_unless_present_any(names)
    }

    /// Sets a conflicting argument by name. I.e. when using this argument,
    /// the following argument can't be present and vice versa.
    ///
    /// **NOTE:** Conflicting rules take precedence over being required by default. Conflict rules
    /// only need to be set for one of the two arguments, they do not need to be set for each.
    ///
    /// **NOTE:** Defining a conflict is two-way, but does *not* need to defined for both arguments
    /// (i.e. if A conflicts with B, defining A.conflicts_with(B) is sufficient. You do not
    /// need to also do B.conflicts_with(A))
    ///
    /// **NOTE:** [`Arg::conflicts_with_all(names)`] allows specifying an argument which conflicts with more than one argument.
    ///
    /// **NOTE** [`Arg::exclusive(true)`] allows specifying an argument which conflicts with every other argument.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use clap::Arg;
    /// Arg::new("config")
    ///     .conflicts_with("debug")
    /// # ;
    /// ```
    ///
    /// Setting conflicting argument, and having both arguments present at runtime is an error.
    ///
    /// ```rust
    /// # use clap::{App, Arg, ErrorKind};
    /// let res = App::new("prog")
    ///     .arg(Arg::new("cfg")
    ///         .takes_value(true)
    ///         .conflicts_with("debug")
    ///         .long("config"))
    ///     .arg(Arg::new("debug")
    ///         .long("debug"))
    ///     .try_get_matches_from(vec![
    ///         "prog", "--debug", "--config", "file.conf"
    ///     ]);
    ///
    /// assert!(res.is_err());
    /// assert_eq!(res.unwrap_err().kind, ErrorKind::ArgumentConflict);
    /// ```
    ///
    /// [`Arg::conflicts_with_all(names)`]: Arg::conflicts_with_all()
    /// [`Arg::exclusive(true)`]: Arg::exclusive()
    pub fn conflicts_with<T: Key>(mut self, arg_id: T) -> Self {
        self.blacklist.push(arg_id.into());
        self
    }

    /// The same as [`Arg::conflicts_with`] but allows specifying multiple two-way conflicts per
    /// argument.
    ///
    /// **NOTE:** Conflicting rules take precedence over being required by default. Conflict rules
    /// only need to be set for one of the two arguments, they do not need to be set for each.
    ///
    /// **NOTE:** Defining a conflict is two-way, but does *not* need to defined for both arguments
    /// (i.e. if A conflicts with B, defining A.conflicts_with(B) is sufficient. You do not need
    /// need to also do B.conflicts_with(A))
    ///
    /// **NOTE:** This option does not exist when using a YAML configuration file. Using [`Arg::conflicts_with`]
    /// followed by an array of strings will achieve the equivalent effect.
    ///
    /// **NOTE:** [`Arg::exclusive(true)`] allows specifying an argument which conflicts with every other argument.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use clap::Arg;
    /// Arg::new("config")
    ///     .conflicts_with_all(&["debug", "input"])
    /// # ;
    /// ```
    ///
    /// Setting conflicting argument, and having any of the arguments present at runtime with a
    /// conflicting argument is an error.
    ///
    /// ```rust
    /// # use clap::{App, Arg, ErrorKind};
    /// let res = App::new("prog")
    ///     .arg(Arg::new("cfg")
    ///         .takes_value(true)
    ///         .conflicts_with_all(&["debug", "input"])
    ///         .long("config"))
    ///     .arg(Arg::new("debug")
    ///         .long("debug"))
    ///     .arg(Arg::new("input")
    ///         .index(1))
    ///     .try_get_matches_from(vec![
    ///         "prog", "--config", "file.conf", "file.txt"
    ///     ]);
    ///
    /// assert!(res.is_err());
    /// assert_eq!(res.unwrap_err().kind, ErrorKind::ArgumentConflict);
    /// ```
    /// [`Arg::conflicts_with`]: Arg::conflicts_with()
    /// [`Arg::exclusive(true)`]: Arg::exclusive()
    pub fn conflicts_with_all(mut self, names: &[&str]) -> Self {
        self.blacklist.extend(names.iter().map(Id::from));
        self
    }

    /// Set an exclusive argument by name. An exclusive argument conflict with every other flag
    /// and must be always passed alone.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use clap::Arg;
    /// Arg::new("config")
    ///     .exclusive(true)
    /// # ;
    /// ```
    ///
    /// If using YAML the above example should be laid out as follows
    ///
    /// ```yaml
    /// - config
    ///     exclusive: true
    /// ```
    ///
    /// Setting an exclusive argument and having any other arguments present at runtime
    /// is an error.
    ///
    /// ```rust
    /// # use clap::{App, Arg, ErrorKind};
    /// let res = App::new("prog")
    ///     .arg(Arg::new("exclusive")
    ///         .takes_value(true)
    ///         .exclusive(true)
    ///         .long("exclusive"))
    ///     .arg(Arg::new("debug")
    ///         .long("debug"))
    ///     .arg(Arg::new("input")
    ///         .index(1))
    ///     .try_get_matches_from(vec![
    ///         "prog", "--exclusive", "file.conf", "file.txt"
    ///     ]);
    ///
    /// assert!(res.is_err());
    /// assert_eq!(res.unwrap_err().kind, ErrorKind::ArgumentConflict);
    /// ```
    #[inline]
    pub fn exclusive(mut self, exclusive: bool) -> Self {
        // FIXME: This should be an ArgSetting, not bool
        self.exclusive = exclusive;
        self
    }

    /// Sets an overridable argument by name. I.e. this argument and the following argument
    /// will override each other in POSIX style (whichever argument was specified at runtime
    /// **last** "wins")
    ///
    /// **NOTE:** When an argument is overridden it is essentially as if it never was used, any
    /// conflicts, requirements, etc. are evaluated **after** all "overrides" have been removed
    ///
    /// **WARNING:** Positional arguments and options which accept
    /// [`ArgSettings::MultipleOccurrences`] cannot override themselves (or we
    /// would never be able to advance to the next positional). If a positional
    /// argument or option with one of the [`ArgSettings::MultipleOccurrences`]
    /// settings lists itself as an override, it is simply ignored.
    ///
    /// # Examples
    ///
    /// ```rust # use clap::{App, Arg};
    /// # use clap::{App, Arg};
    /// let m = App::new("prog")
    ///     .arg(Arg::from("-f, --flag 'some flag'")
    ///         .conflicts_with("debug"))
    ///     .arg(Arg::from("-d, --debug 'other flag'"))
    ///     .arg(Arg::from("-c, --color 'third flag'")
    ///         .overrides_with("flag"))
    ///     .get_matches_from(vec![
    ///         "prog", "-f", "-d", "-c"]);
    ///             //    ^~~~~~~~~~~~^~~~~ flag is overridden by color
    ///
    /// assert!(m.is_present("color"));
    /// assert!(m.is_present("debug")); // even though flag conflicts with debug, it's as if flag
    ///                                 // was never used because it was overridden with color
    /// assert!(!m.is_present("flag"));
    /// ```
    /// Care must be taken when using this setting, and having an arg override with itself. This
    /// is common practice when supporting things like shell aliases, config files, etc.
    /// However, when combined with multiple values, it can get dicy.
    /// Here is how clap handles such situations:
    ///
    /// When a flag overrides itself, it's as if the flag was only ever used once (essentially
    /// preventing a "Unexpected multiple usage" error):
    ///
    /// ```rust
    /// # use clap::{App, Arg};
    /// let m = App::new("posix")
    ///             .arg(Arg::from("--flag  'some flag'").overrides_with("flag"))
    ///             .get_matches_from(vec!["posix", "--flag", "--flag"]);
    /// assert!(m.is_present("flag"));
    /// assert_eq!(m.occurrences_of("flag"), 1);
    /// ```
    ///
    /// Making an arg [`ArgSettings::MultipleOccurrences`] and override itself
    /// is essentially meaningless. Therefore clap ignores an override of self
    /// if it's a flag and it already accepts multiple occurrences.
    ///
    /// ```
    /// # use clap::{App, Arg};
    /// let m = App::new("posix")
    ///             .arg(Arg::from("--flag...  'some flag'").overrides_with("flag"))
    ///             .get_matches_from(vec!["", "--flag", "--flag", "--flag", "--flag"]);
    /// assert!(m.is_present("flag"));
    /// assert_eq!(m.occurrences_of("flag"), 4);
    /// ```
    ///
    /// Now notice with options (which *do not* set
    /// [`ArgSettings::MultipleOccurrences`]), it's as if only the last
    /// occurrence happened.
    ///
    /// ```
    /// # use clap::{App, Arg};
    /// let m = App::new("posix")
    ///             .arg(Arg::from("--opt [val] 'some option'").overrides_with("opt"))
    ///             .get_matches_from(vec!["", "--opt=some", "--opt=other"]);
    /// assert!(m.is_present("opt"));
    /// assert_eq!(m.occurrences_of("opt"), 1);
    /// assert_eq!(m.value_of("opt"), Some("other"));
    /// ```
    ///
    /// This will also work when [`ArgSettings::MultipleValues`] is enabled:
    ///
    /// ```
    /// # use clap::{App, Arg};
    /// let m = App::new("posix")
    ///             .arg(
    ///                 Arg::new("opt")
    ///                     .long("opt")
    ///                     .takes_value(true)
    ///                     .multiple_values(true)
    ///                     .overrides_with("opt")
    ///             )
    ///             .get_matches_from(vec!["", "--opt", "1", "2", "--opt", "3", "4", "5"]);
    /// assert!(m.is_present("opt"));
    /// assert_eq!(m.occurrences_of("opt"), 1);
    /// assert_eq!(m.values_of("opt").unwrap().collect::<Vec<_>>(), &["3", "4", "5"]);
    /// ```
    ///
    /// Just like flags, options with [`ArgSettings::MultipleOccurrences`] set
    /// will ignore the "override self" setting.
    ///
    /// ```
    /// # use clap::{App, Arg};
    /// let m = App::new("posix")
    ///             .arg(Arg::from("--opt [val]... 'some option'")
    ///                 .overrides_with("opt"))
    ///             .get_matches_from(vec!["", "--opt", "first", "over", "--opt", "other", "val"]);
    /// assert!(m.is_present("opt"));
    /// assert_eq!(m.occurrences_of("opt"), 2);
    /// assert_eq!(m.values_of("opt").unwrap().collect::<Vec<_>>(), &["first", "over", "other", "val"]);
    /// ```
    pub fn overrides_with<T: Key>(mut self, arg_id: T) -> Self {
        self.overrides.push(arg_id.into());
        self
    }

    /// Sets multiple mutually overridable arguments by name. I.e. this argument and the following
    /// argument will override each other in POSIX style (whichever argument was specified at
    /// runtime **last** "wins")
    ///
    /// **NOTE:** When an argument is overridden it is essentially as if it never was used, any
    /// conflicts, requirements, etc. are evaluated **after** all "overrides" have been removed
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use clap::{App, Arg};
    /// let m = App::new("prog")
    ///     .arg(Arg::from("-f, --flag 'some flag'")
    ///         .conflicts_with("color"))
    ///     .arg(Arg::from("-d, --debug 'other flag'"))
    ///     .arg(Arg::from("-c, --color 'third flag'")
    ///         .overrides_with_all(&["flag", "debug"]))
    ///     .get_matches_from(vec![
    ///         "prog", "-f", "-d", "-c"]);
    ///             //    ^~~~~~^~~~~~~~~ flag and debug are overridden by color
    ///
    /// assert!(m.is_present("color")); // even though flag conflicts with color, it's as if flag
    ///                                 // and debug were never used because they were overridden
    ///                                 // with color
    /// assert!(!m.is_present("debug"));
    /// assert!(!m.is_present("flag"));
    /// ```
    pub fn overrides_with_all<T: Key>(mut self, names: &[T]) -> Self {
        self.overrides.extend(names.iter().map(Id::from));
        self
    }

    /// Sets an argument by name that is required when this one is present I.e. when
    /// using this argument, the following argument *must* be present.
    ///
    /// **NOTE:** [Conflicting] rules and [override] rules take precedence over being required
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use clap::Arg;
    /// Arg::new("config")
    ///     .requires("input")
    /// # ;
    /// ```
    ///
    /// Setting [`Arg::requires(name)`] requires that the argument be used at runtime if the
    /// defining argument is used. If the defining argument isn't used, the other argument isn't
    /// required
    ///
    /// ```rust
    /// # use clap::{App, Arg};
    /// let res = App::new("prog")
    ///     .arg(Arg::new("cfg")
    ///         .takes_value(true)
    ///         .requires("input")
    ///         .long("config"))
    ///     .arg(Arg::new("input")
    ///         .index(1))
    ///     .try_get_matches_from(vec![
    ///         "prog"
    ///     ]);
    ///
    /// assert!(res.is_ok()); // We didn't use cfg, so input wasn't required
    /// ```
    ///
    /// Setting [`Arg::requires(name)`] and *not* supplying that argument is an error.
    ///
    /// ```rust
    /// # use clap::{App, Arg, ErrorKind};
    /// let res = App::new("prog")
    ///     .arg(Arg::new("cfg")
    ///         .takes_value(true)
    ///         .requires("input")
    ///         .long("config"))
    ///     .arg(Arg::new("input")
    ///         .index(1))
    ///     .try_get_matches_from(vec![
    ///         "prog", "--config", "file.conf"
    ///     ]);
    ///
    /// assert!(res.is_err());
    /// assert_eq!(res.unwrap_err().kind, ErrorKind::MissingRequiredArgument);
    /// ```
    /// [`Arg::requires(name)`]: Arg::requires()
    /// [Conflicting]: Arg::conflicts_with()
    /// [override]: Arg::overrides_with()
    pub fn requires<T: Key>(mut self, arg_id: T) -> Self {
        self.requires.push((None, arg_id.into()));
        self
    }

    /// Require another argument if this arg was present on runtime, and its value equals to `val`.
    ///
    /// This method takes `value, another_arg` pair. At runtime, clap will check
    /// if this arg (`self`) is present and its value equals to `val`.
    /// If it does, `another_arg` will be marked as required.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use clap::Arg;
    /// Arg::new("config")
    ///     .requires_if("val", "arg")
    /// # ;
    /// ```
    ///
    /// If using YAML the values should be laid out as follows
    ///
    /// ```yaml
    /// requires_if:
    ///     - [val, arg]
    /// ```
    ///
    /// Setting `Arg::requires_if(val, arg)` requires that the `arg` be used at runtime if the
    /// defining argument's value is equal to `val`. If the defining argument is anything other than
    /// `val`, the other argument isn't required.
    ///
    /// ```rust
    /// # use clap::{App, Arg};
    /// let res = App::new("prog")
    ///     .arg(Arg::new("cfg")
    ///         .takes_value(true)
    ///         .requires_if("my.cfg", "other")
    ///         .long("config"))
    ///     .arg(Arg::new("other"))
    ///     .try_get_matches_from(vec![
    ///         "prog", "--config", "some.cfg"
    ///     ]);
    ///
    /// assert!(res.is_ok()); // We didn't use --config=my.cfg, so other wasn't required
    /// ```
    ///
    /// Setting `Arg::requires_if(val, arg)` and setting the value to `val` but *not* supplying
    /// `arg` is an error.
    ///
    /// ```rust
    /// # use clap::{App, Arg, ErrorKind};
    /// let res = App::new("prog")
    ///     .arg(Arg::new("cfg")
    ///         .takes_value(true)
    ///         .requires_if("my.cfg", "input")
    ///         .long("config"))
    ///     .arg(Arg::new("input"))
    ///     .try_get_matches_from(vec![
    ///         "prog", "--config", "my.cfg"
    ///     ]);
    ///
    /// assert!(res.is_err());
    /// assert_eq!(res.unwrap_err().kind, ErrorKind::MissingRequiredArgument);
    /// ```
    /// [`Arg::requires(name)`]: Arg::requires()
    /// [Conflicting]: Arg::conflicts_with()
    /// [override]: Arg::overrides_with()
    pub fn requires_if<T: Key>(mut self, val: &'help str, arg_id: T) -> Self {
        self.requires.push((Some(val), arg_id.into()));
        self
    }

    /// Allows multiple conditional requirements. The requirement will only become valid if this arg's value
    /// equals `val`.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use clap::Arg;
    /// Arg::new("config")
    ///     .requires_ifs(&[
    ///         ("val", "arg"),
    ///         ("other_val", "arg2"),
    ///     ])
    /// # ;
    /// ```
    ///
    /// If using YAML the values should be laid out as follows
    ///
    /// ```yaml
    /// requires_if:
    ///     - [val, arg]
    ///     - [val2, arg2]
    /// ```
    ///
    /// Setting `Arg::requires_ifs(&["val", "arg"])` requires that the `arg` be used at runtime if the
    /// defining argument's value is equal to `val`. If the defining argument's value is anything other
    /// than `val`, `arg` isn't required.
    ///
    /// ```rust
    /// # use clap::{App, Arg, ErrorKind};
    /// let res = App::new("prog")
    ///     .arg(Arg::new("cfg")
    ///         .takes_value(true)
    ///         .requires_ifs(&[
    ///             ("special.conf", "opt"),
    ///             ("other.conf", "other"),
    ///         ])
    ///         .long("config"))
    ///     .arg(Arg::new("opt")
    ///         .long("option")
    ///         .takes_value(true))
    ///     .arg(Arg::new("other"))
    ///     .try_get_matches_from(vec![
    ///         "prog", "--config", "special.conf"
    ///     ]);
    ///
    /// assert!(res.is_err()); // We  used --config=special.conf so --option <val> is required
    /// assert_eq!(res.unwrap_err().kind, ErrorKind::MissingRequiredArgument);
    /// ```
    /// [`Arg::requires(name)`]: Arg::requires()
    /// [Conflicting]: Arg::conflicts_with()
    /// [override]: Arg::overrides_with()
    pub fn requires_ifs<T: Key>(mut self, ifs: &[(&'help str, T)]) -> Self {
        self.requires
            .extend(ifs.iter().map(|(val, arg)| (Some(*val), Id::from(arg))));
        self
    }

    /// Allows specifying that this argument is [required] only if the specified
    /// `arg` is present at runtime and its value equals `val`.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use clap::Arg;
    /// Arg::new("config")
    ///     .required_if_eq("other_arg", "value")
    /// # ;
    /// ```
    ///
    /// If using YAML the values should be laid out as follows
    ///
    /// ```yaml
    /// required_if_eq:
    ///     - [arg, val]
    /// ```
    ///
    /// ```rust
    /// # use clap::{App, Arg, ErrorKind};
    /// let res = App::new("prog")
    ///     .arg(Arg::new("cfg")
    ///         .takes_value(true)
    ///         .required_if_eq("other", "special")
    ///         .long("config"))
    ///     .arg(Arg::new("other")
    ///         .long("other")
    ///         .takes_value(true))
    ///     .try_get_matches_from(vec![
    ///         "prog", "--other", "not-special"
    ///     ]);
    ///
    /// assert!(res.is_ok()); // We didn't use --other=special, so "cfg" wasn't required
    ///
    /// let res = App::new("prog")
    ///     .arg(Arg::new("cfg")
    ///         .takes_value(true)
    ///         .required_if_eq("other", "special")
    ///         .long("config"))
    ///     .arg(Arg::new("other")
    ///         .long("other")
    ///         .takes_value(true))
    ///     .try_get_matches_from(vec![
    ///         "prog", "--other", "special"
    ///     ]);
    ///
    /// // We did use --other=special so "cfg" had become required but was missing.
    /// assert!(res.is_err());
    /// assert_eq!(res.unwrap_err().kind, ErrorKind::MissingRequiredArgument);
    ///
    /// let res = App::new("prog")
    ///     .arg(Arg::new("cfg")
    ///         .takes_value(true)
    ///         .required_if_eq("other", "special")
    ///         .long("config"))
    ///     .arg(Arg::new("other")
    ///         .long("other")
    ///         .takes_value(true))
    ///     .try_get_matches_from(vec![
    ///         "prog", "--other", "SPECIAL"
    ///     ]);
    ///
    /// // By default, the comparison is case-sensitive, so "cfg" wasn't required
    /// assert!(res.is_ok());
    ///
    /// let res = App::new("prog")
    ///     .arg(Arg::new("cfg")
    ///         .takes_value(true)
    ///         .required_if_eq("other", "special")
    ///         .long("config"))
    ///     .arg(Arg::new("other")
    ///         .long("other")
    ///         .case_insensitive(true)
    ///         .takes_value(true))
    ///     .try_get_matches_from(vec![
    ///         "prog", "--other", "SPECIAL"
    ///     ]);
    ///
    /// // However, case-insensitive comparisons can be enabled.  This typically occurs when using Arg::possible_values().
    /// assert!(res.is_err());
    /// assert_eq!(res.unwrap_err().kind, ErrorKind::MissingRequiredArgument);
    /// ```
    /// [`Arg::requires(name)`]: Arg::requires()
    /// [Conflicting]: Arg::conflicts_with()
    /// [required]: Arg::required()
    pub fn required_if_eq<T: Key>(mut self, arg_id: T, val: &'help str) -> Self {
        self.r_ifs.push((arg_id.into(), val));
        self
    }

    /// Deprecated, see [`Arg::required_if_eq`]
    #[deprecated(since = "3.0.0", note = "Replaced with `Arg::required_if_eq`")]
    pub fn required_if<T: Key>(self, arg_id: T, val: &'help str) -> Self {
        self.required_if_eq(arg_id, val)
    }

    /// Allows specifying that this argument is [required] based on multiple conditions. The
    /// conditions are set up in a `(arg, val)` style tuple. The requirement will only become valid
    /// if one of the specified `arg`'s value equals its corresponding `val`.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use clap::Arg;
    /// Arg::new("config")
    ///     .required_if_eq_any(&[
    ///         ("extra", "val"),
    ///         ("option", "spec")
    ///     ])
    /// # ;
    /// ```
    ///
    /// If using YAML the values should be laid out as follows
    ///
    /// ```yaml
    /// required_if_eq:
    ///     - [arg, val]
    ///     - [arg2, val2]
    /// ```
    ///
    /// Setting `Arg::required_if_eq_any(&[(arg, val)])` makes this arg required if any of the `arg`s
    /// are used at runtime and it's corresponding value is equal to `val`. If the `arg`'s value is
    /// anything other than `val`, this argument isn't required.
    ///
    /// ```rust
    /// # use clap::{App, Arg};
    /// let res = App::new("prog")
    ///     .arg(Arg::new("cfg")
    ///         .required_if_eq_any(&[
    ///             ("extra", "val"),
    ///             ("option", "spec")
    ///         ])
    ///         .takes_value(true)
    ///         .long("config"))
    ///     .arg(Arg::new("extra")
    ///         .takes_value(true)
    ///         .long("extra"))
    ///     .arg(Arg::new("option")
    ///         .takes_value(true)
    ///         .long("option"))
    ///     .try_get_matches_from(vec![
    ///         "prog", "--option", "other"
    ///     ]);
    ///
    /// assert!(res.is_ok()); // We didn't use --option=spec, or --extra=val so "cfg" isn't required
    /// ```
    ///
    /// Setting `Arg::required_if_eq_any(&[(arg, val)])` and having any of the `arg`s used with its
    /// value of `val` but *not* using this arg is an error.
    ///
    /// ```rust
    /// # use clap::{App, Arg, ErrorKind};
    /// let res = App::new("prog")
    ///     .arg(Arg::new("cfg")
    ///         .required_if_eq_any(&[
    ///             ("extra", "val"),
    ///             ("option", "spec")
    ///         ])
    ///         .takes_value(true)
    ///         .long("config"))
    ///     .arg(Arg::new("extra")
    ///         .takes_value(true)
    ///         .long("extra"))
    ///     .arg(Arg::new("option")
    ///         .takes_value(true)
    ///         .long("option"))
    ///     .try_get_matches_from(vec![
    ///         "prog", "--option", "spec"
    ///     ]);
    ///
    /// assert!(res.is_err());
    /// assert_eq!(res.unwrap_err().kind, ErrorKind::MissingRequiredArgument);
    /// ```
    /// [`Arg::requires(name)`]: Arg::requires()
    /// [Conflicting]: Arg::conflicts_with()
    /// [required]: Arg::required()
    pub fn required_if_eq_any<T: Key>(mut self, ifs: &[(T, &'help str)]) -> Self {
        self.r_ifs
            .extend(ifs.iter().map(|(id, val)| (Id::from_ref(id), *val)));
        self
    }

    /// Deprecated, see [`Arg::required_if_eq_any`]
    #[deprecated(since = "3.0.0", note = "Replaced with `Arg::required_if_eq_any`")]
    pub fn required_ifs<T: Key>(self, ifs: &[(T, &'help str)]) -> Self {
        self.required_if_eq_any(ifs)
    }

    /// Allows specifying that this argument is [required] based on multiple conditions. The
    /// conditions are set up in a `(arg, val)` style tuple. The requirement will only become valid
    /// if every one of the specified `arg`'s value equals its corresponding `val`.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use clap::Arg;
    /// Arg::new("config")
    ///     .required_if_eq_all(&[
    ///         ("extra", "val"),
    ///         ("option", "spec")
    ///     ])
    /// # ;
    /// ```
    ///
    /// If using YAML the values should be laid out as follows
    ///
    /// ```yaml
    /// required_if_eq_all:
    ///     - [arg, val]
    ///     - [arg2, val2]
    /// ```
    ///
    /// Setting `Arg::required_if_eq_all(&[(arg, val)])` makes this arg required if all of the `arg`s
    /// are used at runtime and every value is equal to its corresponding `val`. If the `arg`'s value is
    /// anything other than `val`, this argument isn't required.
    ///
    /// ```rust
    /// # use clap::{App, Arg};
    /// let res = App::new("prog")
    ///     .arg(Arg::new("cfg")
    ///         .required_if_eq_all(&[
    ///             ("extra", "val"),
    ///             ("option", "spec")
    ///         ])
    ///         .takes_value(true)
    ///         .long("config"))
    ///     .arg(Arg::new("extra")
    ///         .takes_value(true)
    ///         .long("extra"))
    ///     .arg(Arg::new("option")
    ///         .takes_value(true)
    ///         .long("option"))
    ///     .try_get_matches_from(vec![
    ///         "prog", "--option", "spec"
    ///     ]);
    ///
    /// assert!(res.is_ok()); // We didn't use --option=spec --extra=val so "cfg" isn't required
    /// ```
    ///
    /// Setting `Arg::required_if_eq_all(&[(arg, val)])` and having all of the `arg`s used with its
    /// value of `val` but *not* using this arg is an error.
    ///
    /// ```rust
    /// # use clap::{App, Arg, ErrorKind};
    /// let res = App::new("prog")
    ///     .arg(Arg::new("cfg")
    ///         .required_if_eq_all(&[
    ///             ("extra", "val"),
    ///             ("option", "spec")
    ///         ])
    ///         .takes_value(true)
    ///         .long("config"))
    ///     .arg(Arg::new("extra")
    ///         .takes_value(true)
    ///         .long("extra"))
    ///     .arg(Arg::new("option")
    ///         .takes_value(true)
    ///         .long("option"))
    ///     .try_get_matches_from(vec![
    ///         "prog", "--extra", "val", "--option", "spec"
    ///     ]);
    ///
    /// assert!(res.is_err());
    /// assert_eq!(res.unwrap_err().kind, ErrorKind::MissingRequiredArgument);
    /// ```
    /// [required]: Arg::required()
    pub fn required_if_eq_all<T: Key>(mut self, ifs: &[(T, &'help str)]) -> Self {
        self.r_ifs_all
            .extend(ifs.iter().map(|(id, val)| (Id::from_ref(id), *val)));
        self
    }

    /// Sets multiple arguments by names that are required when this one is present I.e. when
    /// using this argument, the following arguments *must* be present.
    ///
    /// **NOTE:** [Conflicting] rules and [override] rules take precedence over being required
    /// by default.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use clap::Arg;
    /// Arg::new("config")
    ///     .requires_all(&["input", "output"])
    /// # ;
    /// ```
    ///
    /// Setting `Arg::requires_all(&[arg, arg2])` requires that all the arguments be used at
    /// runtime if the defining argument is used. If the defining argument isn't used, the other
    /// argument isn't required
    ///
    /// ```rust
    /// # use clap::{App, Arg};
    /// let res = App::new("prog")
    ///     .arg(Arg::new("cfg")
    ///         .takes_value(true)
    ///         .requires("input")
    ///         .long("config"))
    ///     .arg(Arg::new("input")
    ///         .index(1))
    ///     .arg(Arg::new("output")
    ///         .index(2))
    ///     .try_get_matches_from(vec![
    ///         "prog"
    ///     ]);
    ///
    /// assert!(res.is_ok()); // We didn't use cfg, so input and output weren't required
    /// ```
    ///
    /// Setting `Arg::requires_all(&[arg, arg2])` and *not* supplying all the arguments is an
    /// error.
    ///
    /// ```rust
    /// # use clap::{App, Arg, ErrorKind};
    /// let res = App::new("prog")
    ///     .arg(Arg::new("cfg")
    ///         .takes_value(true)
    ///         .requires_all(&["input", "output"])
    ///         .long("config"))
    ///     .arg(Arg::new("input")
    ///         .index(1))
    ///     .arg(Arg::new("output")
    ///         .index(2))
    ///     .try_get_matches_from(vec![
    ///         "prog", "--config", "file.conf", "in.txt"
    ///     ]);
    ///
    /// assert!(res.is_err());
    /// // We didn't use output
    /// assert_eq!(res.unwrap_err().kind, ErrorKind::MissingRequiredArgument);
    /// ```
    /// [Conflicting]: Arg::conflicts_with()
    /// [override]: Arg::overrides_with()
    pub fn requires_all<T: Key>(mut self, names: &[T]) -> Self {
        self.requires.extend(names.iter().map(|s| (None, s.into())));
        self
    }

    /// Specifies the index of a positional argument **starting at** 1.
    ///
    /// **NOTE:** The index refers to position according to **other positional argument**. It does
    /// not define position in the argument list as a whole.
    ///
    /// **NOTE:** This is only meant to be used for positional arguments and shouldn't to be used
    /// with [`Arg::short`] or [`Arg::long`].
    ///
    /// **NOTE:** You can optionally leave off the `index` method, and the index will be
    /// assigned in order of evaluation. Utilizing the `index` method allows for setting
    /// indexes out of order
    ///
    /// **NOTE:** When utilized with [`Arg::multiple_values(true)`], only the **last** positional argument
    /// may be defined as multiple (i.e. with the highest index)
    ///
    /// # Panics
    ///
    /// Although not in this method directly, [`App`] will [`panic!`] if indexes are skipped (such
    /// as defining `index(1)` and `index(3)` but not `index(2)`, or a positional argument is
    /// defined as multiple and is not the highest index
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use clap::{App, Arg};
    /// Arg::new("config")
    ///     .index(1)
    /// # ;
    /// ```
    ///
    /// ```rust
    /// # use clap::{App, Arg};
    /// let m = App::new("prog")
    ///     .arg(Arg::new("mode")
    ///         .index(1))
    ///     .arg(Arg::new("debug")
    ///         .long("debug"))
    ///     .get_matches_from(vec![
    ///         "prog", "--debug", "fast"
    ///     ]);
    ///
    /// assert!(m.is_present("mode"));
    /// assert_eq!(m.value_of("mode"), Some("fast")); // notice index(1) means "first positional"
    ///                                               // *not* first argument
    /// ```
    /// [`Arg::short`]: Arg::short()
    /// [`Arg::long`]: Arg::long()
    /// [`Arg::multiple_values(true)`]: Arg::multiple_values()
    /// [`panic!`]: https://doc.rust-lang.org/std/macro.panic!.html
    /// [`App`]: crate::App
    #[inline]
    pub fn index(mut self, idx: usize) -> Self {
        self.index = Some(idx);
        self
    }

    /// Specifies a value that *stops* parsing multiple values of a give argument. By default when
    /// one sets [`multiple_values(true)`] on an argument, clap will continue parsing values for that
    /// argument until it reaches another valid argument, or one of the other more specific settings
    /// for multiple values is used (such as [`min_values`], [`max_values`] or
    /// [`number_of_values`]).
    ///
    /// **NOTE:** This setting only applies to [options] and [positional arguments]
    ///
    /// **NOTE:** When the terminator is passed in on the command line, it is **not** stored as one
    /// of the values
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use clap::{App, Arg};
    /// Arg::new("vals")
    ///     .takes_value(true)
    ///     .multiple_values(true)
    ///     .value_terminator(";")
    /// # ;
    /// ```
    ///
    /// The following example uses two arguments, a sequence of commands, and the location in which
    /// to perform them
    ///
    /// ```rust
    /// # use clap::{App, Arg};
    /// let m = App::new("prog")
    ///     .arg(Arg::new("cmds")
    ///         .takes_value(true)
    ///         .multiple_values(true)
    ///         .allow_hyphen_values(true)
    ///         .value_terminator(";"))
    ///     .arg(Arg::new("location"))
    ///     .get_matches_from(vec![
    ///         "prog", "find", "-type", "f", "-name", "special", ";", "/home/clap"
    ///     ]);
    /// let cmds: Vec<_> = m.values_of("cmds").unwrap().collect();
    /// assert_eq!(&cmds, &["find", "-type", "f", "-name", "special"]);
    /// assert_eq!(m.value_of("location"), Some("/home/clap"));
    /// ```
    /// [options]: Arg::takes_value()
    /// [positional arguments]: Arg::index()
    /// [`multiple_values(true)`]: Arg::multiple_values()
    /// [`min_values`]: Arg::min_values()
    /// [`number_of_values`]: Arg::number_of_values()
    /// [`max_values`]: Arg::max_values()
    #[inline]
    pub fn value_terminator(mut self, term: &'help str) -> Self {
        self.terminator = Some(term);
        self.takes_value(true)
    }

    /// Specifies a list of possible values for this argument. At runtime, `clap` verifies that
    /// only one of the specified values was used, or fails with an error message.
    ///
    /// **NOTE:** This setting only applies to [options] and [positional arguments]
    ///
    /// **NOTE:** You can use both strings directly or use [`PossibleValue`] if you want more control
    /// over single possible values.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use clap::{App, Arg};
    /// Arg::new("mode")
    ///     .takes_value(true)
    ///     .possible_values(["fast", "slow", "medium"])
    /// # ;
    /// ```
    /// The same using [`PossibleValue`]:
    ///
    /// ```rust
    /// # use clap::{App, Arg, PossibleValue};
    /// Arg::new("mode").takes_value(true).possible_values([
    ///     PossibleValue::new("fast"),
    /// // value with a help text
    ///     PossibleValue::new("slow").about("not that fast"),
    /// // value that is hidden from completion and help text
    ///     PossibleValue::new("medium").hidden(true),
    /// ])
    /// # ;
    /// ```
    ///
    /// ```rust
    /// # use clap::{App, Arg};
    /// let m = App::new("prog")
    ///     .arg(Arg::new("mode")
    ///         .long("mode")
    ///         .takes_value(true)
    ///         .possible_values(["fast", "slow", "medium"]))
    ///     .get_matches_from(vec![
    ///         "prog", "--mode", "fast"
    ///     ]);
    /// assert!(m.is_present("mode"));
    /// assert_eq!(m.value_of("mode"), Some("fast"));
    /// ```
    ///
    /// The next example shows a failed parse from using a value which wasn't defined as one of the
    /// possible values.
    ///
    /// ```rust
    /// # use clap::{App, Arg, ErrorKind};
    /// let res = App::new("prog")
    ///     .arg(Arg::new("mode")
    ///         .long("mode")
    ///         .takes_value(true)
    ///         .possible_values(["fast", "slow", "medium"]))
    ///     .try_get_matches_from(vec![
    ///         "prog", "--mode", "wrong"
    ///     ]);
    /// assert!(res.is_err());
    /// assert_eq!(res.unwrap_err().kind, ErrorKind::InvalidValue);
    /// ```
    /// [options]: Arg::takes_value()
    /// [positional arguments]: Arg::index()
    pub fn possible_values<I, T>(mut self, values: I) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Into<PossibleValue<'help>>,
    {
        self.possible_vals
            .extend(values.into_iter().map(|value| value.into()));
        self.takes_value(true)
    }

    /// Specifies a possible value for this argument, one at a time. At runtime, `clap` verifies
    /// that only one of the specified values was used, or fails with error message.
    ///
    /// **NOTE:** This setting only applies to [options] and [positional arguments]
    ///
    /// **NOTE:** You can use both strings directly or use [`PossibleValue`] if you want more control
    /// over single possible values.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use clap::{App, Arg};
    /// Arg::new("mode")
    ///     .takes_value(true)
    ///     .possible_value("fast")
    ///     .possible_value("slow")
    ///     .possible_value("medium")
    /// # ;
    /// ```
    /// The same using [`PossibleValue`]:
    ///
    /// ```rust
    /// # use clap::{App, Arg, PossibleValue};
    /// Arg::new("mode").takes_value(true)
    ///     .possible_value(PossibleValue::new("fast"))
    /// // value with a help text
    ///     .possible_value(PossibleValue::new("slow").about("not that fast"))
    /// // value that is hidden from completion and help text
    ///     .possible_value(PossibleValue::new("medium").hidden(true))
    /// # ;
    /// ```
    ///
    /// ```rust
    /// # use clap::{App, Arg};
    /// let m = App::new("prog")
    ///     .arg(Arg::new("mode")
    ///         .long("mode")
    ///         .takes_value(true)
    ///         .possible_value("fast")
    ///         .possible_value("slow")
    ///         .possible_value("medium"))
    ///     .get_matches_from(vec![
    ///         "prog", "--mode", "fast"
    ///     ]);
    /// assert!(m.is_present("mode"));
    /// assert_eq!(m.value_of("mode"), Some("fast"));
    /// ```
    ///
    /// The next example shows a failed parse from using a value which wasn't defined as one of the
    /// possible values.
    ///
    /// ```rust
    /// # use clap::{App, Arg, ErrorKind};
    /// let res = App::new("prog")
    ///     .arg(Arg::new("mode")
    ///         .long("mode")
    ///         .takes_value(true)
    ///         .possible_value("fast")
    ///         .possible_value("slow")
    ///         .possible_value("medium"))
    ///     .try_get_matches_from(vec![
    ///         "prog", "--mode", "wrong"
    ///     ]);
    /// assert!(res.is_err());
    /// assert_eq!(res.unwrap_err().kind, ErrorKind::InvalidValue);
    /// ```
    /// [options]: Arg::takes_value()
    /// [positional arguments]: Arg::index()
    pub fn possible_value<T>(mut self, value: T) -> Self
    where
        T: Into<PossibleValue<'help>>,
    {
        self.possible_vals.push(value.into());
        self.takes_value(true)
    }

    /// Specifies the name of the [`ArgGroup`] the argument belongs to.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use clap::{App, Arg};
    /// Arg::new("debug")
    ///     .long("debug")
    ///     .group("mode")
    /// # ;
    /// ```
    ///
    /// Multiple arguments can be a member of a single group and then the group checked as if it
    /// was one of said arguments.
    ///
    /// ```rust
    /// # use clap::{App, Arg};
    /// let m = App::new("prog")
    ///     .arg(Arg::new("debug")
    ///         .long("debug")
    ///         .group("mode"))
    ///     .arg(Arg::new("verbose")
    ///         .long("verbose")
    ///         .group("mode"))
    ///     .get_matches_from(vec![
    ///         "prog", "--debug"
    ///     ]);
    /// assert!(m.is_present("mode"));
    /// ```
    ///
    /// [`ArgGroup`]: crate::ArgGroup
    pub fn group<T: Key>(mut self, group_id: T) -> Self {
        self.groups.push(group_id.into());
        self
    }

    /// Specifies the names of multiple [`ArgGroup`]'s the argument belongs to.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use clap::{App, Arg};
    /// Arg::new("debug")
    ///     .long("debug")
    ///     .groups(&["mode", "verbosity"])
    /// # ;
    /// ```
    ///
    /// Arguments can be members of multiple groups and then the group checked as if it
    /// was one of said arguments.
    ///
    /// ```rust
    /// # use clap::{App, Arg};
    /// let m = App::new("prog")
    ///     .arg(Arg::new("debug")
    ///         .long("debug")
    ///         .groups(&["mode", "verbosity"]))
    ///     .arg(Arg::new("verbose")
    ///         .long("verbose")
    ///         .groups(&["mode", "verbosity"]))
    ///     .get_matches_from(vec![
    ///         "prog", "--debug"
    ///     ]);
    /// assert!(m.is_present("mode"));
    /// assert!(m.is_present("verbosity"));
    /// ```
    ///
    /// [`ArgGroup`]: crate::ArgGroup
    pub fn groups<T: Key>(mut self, group_ids: &[T]) -> Self {
        self.groups.extend(group_ids.iter().map(Id::from));
        self
    }

    /// Specifies how many values are required to satisfy this argument. For example, if you had a
    /// `-f <file>` argument where you wanted exactly 3 'files' you would set
    /// `.number_of_values(3)`, and this argument wouldn't be satisfied unless the user provided
    /// 3 and only 3 values.
    ///
    /// **NOTE:** Does *not* require [`Arg::multiple_occurrences(true)`] to be set. Setting
    /// [`Arg::multiple_occurrences(true)`] would allow `-f <file> <file> <file> -f <file> <file> <file>` where
    /// as *not* setting it would only allow one occurrence of this argument.
    ///
    /// **NOTE:** implicitly sets [`Arg::takes_value(true)`] and [`Arg::multiple_values(true)`].
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use clap::{App, Arg};
    /// Arg::new("file")
    ///     .short('f')
    ///     .number_of_values(3);
    /// ```
    ///
    /// Not supplying the correct number of values is an error
    ///
    /// ```rust
    /// # use clap::{App, Arg, ErrorKind};
    /// let res = App::new("prog")
    ///     .arg(Arg::new("file")
    ///         .takes_value(true)
    ///         .number_of_values(2)
    ///         .short('F'))
    ///     .try_get_matches_from(vec![
    ///         "prog", "-F", "file1"
    ///     ]);
    ///
    /// assert!(res.is_err());
    /// assert_eq!(res.unwrap_err().kind, ErrorKind::WrongNumberOfValues);
    /// ```
    /// [`Arg::multiple_occurrences(true)`]: Arg::multiple_occurrences()
    #[inline]
    pub fn number_of_values(mut self, qty: usize) -> Self {
        self.num_vals = Some(qty);
        self.takes_value(true).multiple_values(true)
    }

    /// Specifies that option values that are invalid UTF-8 should *not* be treated as an error.
    ///
    /// **NOTE:** Using argument values with invalid UTF-8 code points requires using
    /// [`ArgMatches::value_of_os`], [`ArgMatches::values_of_os`], [`ArgMatches::value_of_lossy`],
    /// or [`ArgMatches::values_of_lossy`] for those particular arguments which may contain invalid
    /// UTF-8 values.
    ///
    /// **NOTE:** Setting this requires [`ArgSettings::TakesValue`]
    ///
    /// # Examples
    ///
    #[cfg_attr(not(unix), doc = " ```ignore")]
    #[cfg_attr(unix, doc = " ```rust")]
    /// # use clap::{App, Arg};
    /// use std::ffi::OsString;
    /// use std::os::unix::ffi::{OsStrExt,OsStringExt};
    /// let r = App::new("myprog")
    ///     .arg(Arg::new("arg").allow_invalid_utf8(true))
    ///     .try_get_matches_from(vec![
    ///         OsString::from("myprog"),
    ///         OsString::from_vec(vec![0xe9])
    ///     ]);
    ///
    /// assert!(r.is_ok());
    /// let m = r.unwrap();
    /// assert_eq!(m.value_of_os("arg").unwrap().as_bytes(), &[0xe9]);
    /// ```
    /// [`ArgMatches::value_of_os`]: crate::ArgMatches::value_of_os()
    /// [`ArgMatches::values_of_os`]: crate::ArgMatches::values_of_os()
    /// [`ArgMatches::value_of_lossy`]: crate::ArgMatches::value_of_lossy()
    /// [`ArgMatches::values_of_lossy`]: crate::ArgMatches::values_of_lossy()
    #[inline]
    pub fn allow_invalid_utf8(self, tv: bool) -> Self {
        if tv {
            self.setting(ArgSettings::AllowInvalidUtf8)
        } else {
            self.unset_setting(ArgSettings::AllowInvalidUtf8)
        }
    }

    /// Allows one to perform a custom validation on the argument value. You provide a closure
    /// which accepts a [`String`] value, and return a [`Result`] where the [`Err(String)`] is a
    /// message displayed to the user.
    ///
    /// **NOTE:** The error message does *not* need to contain the `error:` portion, only the
    /// message as all errors will appear as
    /// `error: Invalid value for '<arg>': <YOUR MESSAGE>` where `<arg>` is replaced by the actual
    /// arg, and `<YOUR MESSAGE>` is the `String` you return as the error.
    ///
    /// **NOTE:** There is a small performance hit for using validators, as they are implemented
    /// with [`Arc`] pointers. And the value to be checked will be allocated an extra time in order
    /// to be passed to the closure. This performance hit is extremely minimal in the grand
    /// scheme of things.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use clap::{App, Arg};
    /// fn has_at(v: &str) -> Result<(), String> {
    ///     if v.contains("@") { return Ok(()); }
    ///     Err(String::from("The value did not contain the required @ sigil"))
    /// }
    /// let res = App::new("prog")
    ///     .arg(Arg::new("file")
    ///         .index(1)
    ///         .validator(has_at))
    ///     .try_get_matches_from(vec![
    ///         "prog", "some@file"
    ///     ]);
    /// assert!(res.is_ok());
    /// assert_eq!(res.unwrap().value_of("file"), Some("some@file"));
    /// ```
    /// [`String`]: std::string::String
    /// [`Result`]: std::result::Result
    /// [`Err(String)`]: std::result::Result::Err
    /// [`Arc`]: std::sync::Arc
    pub fn validator<F, O, E>(mut self, mut f: F) -> Self
    where
        F: FnMut(&str) -> Result<O, E> + Send + 'help,
        E: Into<Box<dyn Error + Send + Sync + 'static>>,
    {
        self.validator = Some(Arc::new(Mutex::new(move |s: &str| {
            f(s).map(|_| ()).map_err(|e| e.into())
        })));
        self
    }

    /// Works identically to Validator but is intended to be used with values that could
    /// contain non UTF-8 formatted strings.
    ///
    /// # Examples
    ///
    #[cfg_attr(not(unix), doc = " ```ignore")]
    #[cfg_attr(unix, doc = " ```rust")]
    /// # use clap::{App, Arg};
    /// # use std::ffi::{OsStr, OsString};
    /// # use std::os::unix::ffi::OsStrExt;
    /// fn has_ampersand(v: &OsStr) -> Result<(), String> {
    ///     if v.as_bytes().iter().any(|b| *b == b'&') { return Ok(()); }
    ///     Err(String::from("The value did not contain the required & sigil"))
    /// }
    /// let res = App::new("prog")
    ///     .arg(Arg::new("file")
    ///         .index(1)
    ///         .validator_os(has_ampersand))
    ///     .try_get_matches_from(vec![
    ///         "prog", "Fish & chips"
    ///     ]);
    /// assert!(res.is_ok());
    /// assert_eq!(res.unwrap().value_of("file"), Some("Fish & chips"));
    /// ```
    /// [`String`]: std::string::String
    /// [`OsStr`]: std::ffi::OsStr
    /// [`OsString`]: std::ffi::OsString
    /// [`Result`]: std::result::Result
    /// [`Err(String)`]: std::result::Result::Err
    /// [`Rc`]: std::rc::Rc
    pub fn validator_os<F, O, E>(mut self, mut f: F) -> Self
    where
        F: FnMut(&OsStr) -> Result<O, E> + Send + 'help,
        E: Into<Box<dyn Error + Send + Sync + 'static>>,
    {
        self.validator_os = Some(Arc::new(Mutex::new(move |s: &OsStr| {
            f(s).map(|_| ()).map_err(|e| e.into())
        })));
        self
    }

    /// Validates the argument via the given regular expression.
    ///
    /// As regular expressions are not very user friendly, the additional `err_message` should
    /// describe the expected format in clear words. All notes for [`Arg::validator()`] regarding the
    /// error message and performance also hold for `validator_regex`.
    ///
    /// The regular expression can either be borrowed or moved into `validator_regex`. This happens
    /// automatically via [`RegexRef`]'s `Into` implementation.
    ///
    /// # Performance
    /// Regular expressions are expensive to compile. You should prefer sharing your regular expression.
    /// We use a [`Cow`]-like internal structure to enable both sharing as well as taking ownership of a
    /// provided regular expression.
    ///
    /// # Examples
    ///
    /// You can use the classical `"\d+"` regular expression to match digits only:
    ///
    /// ```rust
    /// # use clap::{App, Arg};
    /// use regex::Regex;
    ///
    /// let digits = Regex::new(r"\d+").unwrap();
    ///
    /// let res = App::new("prog")
    ///     .arg(Arg::new("digits")
    ///         .index(1)
    ///         .validator_regex(&digits, "only digits are allowed"))
    ///     .try_get_matches_from(vec![
    ///         "prog", "12345"
    ///     ]);
    /// assert!(res.is_ok());
    /// assert_eq!(res.unwrap().value_of("digits"), Some("12345"));
    /// ```
    ///
    /// However, any valid `Regex` can be used:
    ///
    /// ```rust
    /// # use clap::{App, Arg, ErrorKind};
    /// use regex::Regex;
    ///
    /// let priority = Regex::new(r"[A-C]").unwrap();
    ///
    /// let res = App::new("prog")
    ///     .arg(Arg::new("priority")
    ///         .index(1)
    ///         .validator_regex(priority, "only priorities A, B or C are allowed"))
    ///     .try_get_matches_from(vec![
    ///         "prog", "12345"
    ///     ]);
    /// assert!(res.is_err());
    /// assert_eq!(res.err().unwrap().kind, ErrorKind::ValueValidation)
    /// ```
    ///
    /// If using YAML then a single vector with two entries should be provided:
    /// ```yaml
    /// validator_regex: [remove-all-files, needs the exact phrase 'remove-all-files' to continue]
    /// ```
    #[cfg(feature = "regex")]
    pub fn validator_regex(
        self,
        regex: impl Into<RegexRef<'help>>,
        err_message: &'help str,
    ) -> Self {
        let regex = regex.into();
        self.validator(move |s: &str| {
            if regex.is_match(s) {
                Ok(())
            } else {
                Err(err_message)
            }
        })
    }

    /// Specifies the *maximum* number of occurrences for this argument. For example, if you had a
    /// `-v` flag and you wanted up to 3 levels of verbosity you would set `.max_occurrences(3)`, and
    /// this argument would be satisfied if the user provided it once or twice or thrice.
    ///
    /// **NOTE:** This implicitly sets [`Arg::multiple_occurrences(true)`] if the value is greater than 1.
    /// # Examples
    ///
    /// ```rust
    /// # use clap::{App, Arg};
    /// Arg::new("verbosity")
    ///     .short('v')
    ///     .max_occurrences(3);
    /// ```
    ///
    /// Supplying less than the maximum number of arguments is allowed
    ///
    /// ```rust
    /// # use clap::{App, Arg};
    /// let res = App::new("prog")
    ///     .arg(Arg::new("verbosity")
    ///         .max_occurrences(3)
    ///         .short('v'))
    ///     .try_get_matches_from(vec![
    ///         "prog", "-vvv"
    ///     ]);
    ///
    /// assert!(res.is_ok());
    /// let m = res.unwrap();
    /// assert_eq!(m.occurrences_of("verbosity"), 3);
    /// ```
    ///
    /// Supplying more than the maximum number of arguments is an error
    ///
    /// ```rust
    /// # use clap::{App, Arg, ErrorKind};
    /// let res = App::new("prog")
    ///     .arg(Arg::new("verbosity")
    ///         .max_occurrences(2)
    ///         .short('v'))
    ///     .try_get_matches_from(vec![
    ///         "prog", "-vvv"
    ///     ]);
    ///
    /// assert!(res.is_err());
    /// assert_eq!(res.unwrap_err().kind, ErrorKind::TooManyOccurrences);
    /// ```
    /// [`Arg::multiple_occurrences(true)`]: Arg::multiple_occurrences()
    #[inline]
    pub fn max_occurrences(mut self, qty: usize) -> Self {
        self.max_occurs = Some(qty);
        if qty > 1 {
            self.multiple_occurrences(true)
        } else {
            self
        }
    }

    /// Specifies the *maximum* number of values are for this argument. For example, if you had a
    /// `-f <file>` argument where you wanted up to 3 'files' you would set `.max_values(3)`, and
    /// this argument would be satisfied if the user provided, 1, 2, or 3 values.
    ///
    /// **NOTE:** This does *not* implicitly set [`Arg::multiple_occurrences(true)`]. This is because
    /// `-o val -o val` is multiple occurrences but a single value and `-o val1 val2` is a single
    /// occurrence with multiple values. For positional arguments this **does** set
    /// [`Arg::multiple_occurrences(true)`] because there is no way to determine the difference between multiple
    /// occurrences and multiple values.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use clap::{App, Arg};
    /// Arg::new("file")
    ///     .short('f')
    ///     .max_values(3);
    /// ```
    ///
    /// Supplying less than the maximum number of values is allowed
    ///
    /// ```rust
    /// # use clap::{App, Arg};
    /// let res = App::new("prog")
    ///     .arg(Arg::new("file")
    ///         .takes_value(true)
    ///         .max_values(3)
    ///         .short('F'))
    ///     .try_get_matches_from(vec![
    ///         "prog", "-F", "file1", "file2"
    ///     ]);
    ///
    /// assert!(res.is_ok());
    /// let m = res.unwrap();
    /// let files: Vec<_> = m.values_of("file").unwrap().collect();
    /// assert_eq!(files, ["file1", "file2"]);
    /// ```
    ///
    /// Supplying more than the maximum number of values is an error
    ///
    /// ```rust
    /// # use clap::{App, Arg, ErrorKind};
    /// let res = App::new("prog")
    ///     .arg(Arg::new("file")
    ///         .takes_value(true)
    ///         .max_values(2)
    ///         .short('F'))
    ///     .try_get_matches_from(vec![
    ///         "prog", "-F", "file1", "file2", "file3"
    ///     ]);
    ///
    /// assert!(res.is_err());
    /// assert_eq!(res.unwrap_err().kind, ErrorKind::UnknownArgument);
    /// ```
    /// [`Arg::multiple_occurrences(true)`]: Arg::multiple_occurrences()
    #[inline]
    pub fn max_values(mut self, qty: usize) -> Self {
        self.max_vals = Some(qty);
        self.takes_value(true).multiple_values(true)
    }

    /// Specifies the *minimum* number of values for this argument. For example, if you had a
    /// `-f <file>` argument where you wanted at least 2 'files' you would set
    /// `.min_values(2)`, and this argument would be satisfied if the user provided, 2 or more
    /// values.
    ///
    /// **NOTE:** This does not implicitly set [`Arg::multiple_occurrences(true)`]. This is because
    /// `-o val -o val` is multiple occurrences but a single value and `-o val1 val2` is a single
    /// occurrence with multiple values. For positional arguments this **does** set
    /// [`Arg::multiple_occurrences(true)`] because there is no way to determine the difference between multiple
    /// occurrences and multiple values.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use clap::{App, Arg};
    /// Arg::new("file")
    ///     .short('f')
    ///     .min_values(3);
    /// ```
    ///
    /// Supplying more than the minimum number of values is allowed
    ///
    /// ```rust
    /// # use clap::{App, Arg};
    /// let res = App::new("prog")
    ///     .arg(Arg::new("file")
    ///         .takes_value(true)
    ///         .min_values(2)
    ///         .short('F'))
    ///     .try_get_matches_from(vec![
    ///         "prog", "-F", "file1", "file2", "file3"
    ///     ]);
    ///
    /// assert!(res.is_ok());
    /// let m = res.unwrap();
    /// let files: Vec<_> = m.values_of("file").unwrap().collect();
    /// assert_eq!(files, ["file1", "file2", "file3"]);
    /// ```
    ///
    /// Supplying less than the minimum number of values is an error
    ///
    /// ```rust
    /// # use clap::{App, Arg, ErrorKind};
    /// let res = App::new("prog")
    ///     .arg(Arg::new("file")
    ///         .takes_value(true)
    ///         .min_values(2)
    ///         .short('F'))
    ///     .try_get_matches_from(vec![
    ///         "prog", "-F", "file1"
    ///     ]);
    ///
    /// assert!(res.is_err());
    /// assert_eq!(res.unwrap_err().kind, ErrorKind::TooFewValues);
    /// ```
    /// [`Arg::multiple_occurrences(true)`]: Arg::multiple_occurrences()
    #[inline]
    pub fn min_values(mut self, qty: usize) -> Self {
        self.min_vals = Some(qty);
        self.takes_value(true).multiple_values(true)
    }

    /// Specifies the separator to use when values are clumped together, defaults to `,` (comma).
    ///
    /// **NOTE:** implicitly sets [`Arg::use_delimiter(true)`]
    ///
    /// **NOTE:** implicitly sets [`Arg::takes_value(true)`]
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use clap::{App, Arg};
    /// let m = App::new("prog")
    ///     .arg(Arg::new("config")
    ///         .short('c')
    ///         .long("config")
    ///         .value_delimiter(';'))
    ///     .get_matches_from(vec![
    ///         "prog", "--config=val1;val2;val3"
    ///     ]);
    ///
    /// assert_eq!(m.values_of("config").unwrap().collect::<Vec<_>>(), ["val1", "val2", "val3"])
    /// ```
    /// [`Arg::use_delimiter(true)`]: Arg::use_delimiter()
    /// [`Arg::takes_value(true)`]: Arg::takes_value()
    #[inline]
    pub fn value_delimiter(mut self, d: char) -> Self {
        self.val_delim = Some(d);
        self.takes_value(true).use_delimiter(true)
    }

    /// Specify multiple names for values of option arguments. These names are cosmetic only, used
    /// for help and usage strings only. The names are **not** used to access arguments. The values
    /// of the arguments are accessed in numeric order (i.e. if you specify two names `one` and
    /// `two` `one` will be the first matched value, `two` will be the second).
    ///
    /// This setting can be very helpful when describing the type of input the user should be
    /// using, such as `FILE`, `INTERFACE`, etc. Although not required, it's somewhat convention to
    /// use all capital letters for the value name.
    ///
    /// **Pro Tip:** It may help to use [`Arg::next_line_help(true)`] if there are long, or
    /// multiple value names in order to not throw off the help text alignment of all options.
    ///
    /// **NOTE:** implicitly sets [`Arg::takes_value(true)`] and [`Arg::multiple_values(true)`].
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use clap::{App, Arg};
    /// Arg::new("speed")
    ///     .short('s')
    ///     .value_names(&["fast", "slow"]);
    /// ```
    ///
    /// ```rust
    /// # use clap::{App, Arg};
    /// let m = App::new("prog")
    ///     .arg(Arg::new("io")
    ///         .long("io-files")
    ///         .value_names(&["INFILE", "OUTFILE"]))
    ///     .get_matches_from(vec![
    ///         "prog", "--help"
    ///     ]);
    /// ```
    ///
    /// Running the above program produces the following output
    ///
    /// ```text
    /// valnames
    ///
    /// USAGE:
    ///    valnames [OPTIONS]
    ///
    /// OPTIONS:
    ///     -h, --help                       Print help information
    ///     --io-files <INFILE> <OUTFILE>    Some help text
    ///     -V, --version                    Print version information
    /// ```
    /// [`Arg::next_line_help(true)`]: Arg::next_line_help()
    /// [`Arg::number_of_values`]: Arg::number_of_values()
    /// [`Arg::takes_value(true)`]: Arg::takes_value()
    /// [`Arg::multiple_values(true)`]: Arg::multiple_values()
    pub fn value_names(mut self, names: &[&'help str]) -> Self {
        self.val_names = names.to_vec();
        self.takes_value(true)
    }

    /// Specifies the name for value of [option] or [positional] arguments inside of help
    /// documentation. This name is cosmetic only, the name is **not** used to access arguments.
    /// This setting can be very helpful when describing the type of input the user should be
    /// using, such as `FILE`, `INTERFACE`, etc. Although not required, it's somewhat convention to
    /// use all capital letters for the value name.
    ///
    /// **NOTE:** implicitly sets [`Arg::takes_value(true)`]
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use clap::{App, Arg};
    /// Arg::new("cfg")
    ///     .long("config")
    ///     .value_name("FILE")
    /// # ;
    /// ```
    ///
    /// ```rust
    /// # use clap::{App, Arg};
    /// let m = App::new("prog")
    ///     .arg(Arg::new("config")
    ///         .long("config")
    ///         .value_name("FILE")
    ///         .about("Some help text"))
    ///     .get_matches_from(vec![
    ///         "prog", "--help"
    ///     ]);
    /// ```
    /// Running the above program produces the following output
    ///
    /// ```text
    /// valnames
    ///
    /// USAGE:
    ///    valnames [OPTIONS]
    ///
    /// OPTIONS:
    ///     --config <FILE>     Some help text
    ///     -h, --help          Print help information
    ///     -V, --version       Print version information
    /// ```
    /// [option]: Arg::takes_value()
    /// [positional]: Arg::index()
    /// [`Arg::takes_value(true)`]: Arg::takes_value()
    #[inline]
    pub fn value_name(self, name: &'help str) -> Self {
        self.value_names(&[name])
    }

    /// Specifies the value of the argument when *not* specified at runtime.
    ///
    /// **NOTE:** If the user *does not* use this argument at runtime, [`ArgMatches::occurrences_of`]
    /// will return `0` even though the [`ArgMatches::value_of`] will return the default specified.
    ///
    /// **NOTE:** If the user *does not* use this argument at runtime [`ArgMatches::is_present`] will
    /// still return `true`. If you wish to determine whether the argument was used at runtime or
    /// not, consider [`ArgMatches::occurrences_of`] which will return `0` if the argument was *not*
    /// used at runtime.
    ///
    /// **NOTE:** This setting is perfectly compatible with [`Arg::default_value_if`] but slightly
    /// different. `Arg::default_value` *only* takes effect when the user has not provided this arg
    /// at runtime. `Arg::default_value_if` however only takes effect when the user has not provided
    /// a value at runtime **and** these other conditions are met as well. If you have set
    /// `Arg::default_value` and `Arg::default_value_if`, and the user **did not** provide this arg
    /// at runtime, nor were the conditions met for `Arg::default_value_if`, the `Arg::default_value`
    /// will be applied.
    ///
    /// **NOTE:** This implicitly sets [`Arg::takes_value(true)`].
    ///
    /// **NOTE:** This setting effectively disables `AppSettings::ArgRequiredElseHelp` if used in
    /// conjunction as it ensures that some argument will always be present.
    ///
    /// # Examples
    ///
    /// First we use the default value without providing any value at runtime.
    ///
    /// ```rust
    /// # use clap::{App, Arg};
    /// let m = App::new("prog")
    ///     .arg(Arg::new("opt")
    ///         .long("myopt")
    ///         .default_value("myval"))
    ///     .get_matches_from(vec![
    ///         "prog"
    ///     ]);
    ///
    /// assert_eq!(m.value_of("opt"), Some("myval"));
    /// assert!(m.is_present("opt"));
    /// assert_eq!(m.occurrences_of("opt"), 0);
    /// ```
    ///
    /// Next we provide a value at runtime to override the default.
    ///
    /// ```rust
    /// # use clap::{App, Arg};
    /// let m = App::new("prog")
    ///     .arg(Arg::new("opt")
    ///         .long("myopt")
    ///         .default_value("myval"))
    ///     .get_matches_from(vec![
    ///         "prog", "--myopt=non_default"
    ///     ]);
    ///
    /// assert_eq!(m.value_of("opt"), Some("non_default"));
    /// assert!(m.is_present("opt"));
    /// assert_eq!(m.occurrences_of("opt"), 1);
    /// ```
    /// [`ArgMatches::occurrences_of`]: crate::ArgMatches::occurrences_of()
    /// [`ArgMatches::value_of`]: crate::ArgMatches::value_of()
    /// [`Arg::takes_value(true)`]: Arg::takes_value()
    /// [`ArgMatches::is_present`]: crate::ArgMatches::is_present()
    /// [`Arg::default_value_if`]: Arg::default_value_if()
    #[inline]
    pub fn default_value(self, val: &'help str) -> Self {
        self.default_values_os(&[OsStr::new(val)])
    }

    /// Provides a default value in the exact same manner as [`Arg::default_value`]
    /// only using [`OsStr`]s instead.
    ///
    /// [`Arg::default_value`]: Arg::default_value()
    /// [`OsStr`]: std::ffi::OsStr
    #[inline]
    pub fn default_value_os(self, val: &'help OsStr) -> Self {
        self.default_values_os(&[val])
    }

    /// Like [`Arg::default_value`] but for args taking multiple values
    ///
    /// [`Arg::default_value`]: Arg::default_value()
    #[inline]
    pub fn default_values(self, vals: &[&'help str]) -> Self {
        let vals_vec: Vec<_> = vals.iter().map(|val| OsStr::new(*val)).collect();
        self.default_values_os(&vals_vec[..])
    }

    /// Provides default values in the exact same manner as [`Arg::default_values`]
    /// only using [`OsStr`]s instead.
    ///
    /// [`Arg::default_values`]: Arg::default_values()
    /// [`OsStr`]: std::ffi::OsStr
    #[inline]
    pub fn default_values_os(mut self, vals: &[&'help OsStr]) -> Self {
        self.default_vals = vals.to_vec();
        self.takes_value(true)
    }

    /// Specifies a value for the argument when the argument is supplied and a value is required
    /// but the value is *not* specified at runtime.
    ///
    /// This configuration option is often used to give the user a shortcut and allow them to
    /// efficiently specify an option argument without requiring an explicitly value. The `--color`
    /// argument is a common example. By, supplying an default, such as `default_missing_value("always")`,
    /// the user can quickly just add `--color` to the command line to produce the desired color output.
    ///
    /// **NOTE:** using this configuration option requires the use of the `.min_values(0)` and the
    /// `.require_equals(true)` configuration option. These are required in order to unambiguously
    /// determine what, if any, value was supplied for the argument.
    ///
    /// # Examples
    ///
    /// Here is an implementation of the common POSIX style `--color` argument.
    ///
    /// ```rust
    /// # use clap::{App, Arg};
    ///
    /// macro_rules! app {
    ///     () => {{
    ///         App::new("prog")
    ///             .arg(Arg::new("color").long("color")
    ///                 .value_name("WHEN")
    ///                 .possible_values(["always", "auto", "never"])
    ///                 .default_value("auto")
    ///                 .overrides_with("color")
    ///                 .min_values(0)
    ///                 .require_equals(true)
    ///                 .default_missing_value("always")
    ///                 .about("Specify WHEN to colorize output.")
    ///             )
    ///    }};
    /// }
    ///
    /// let mut m;
    ///
    /// // first, we'll provide no arguments
    ///
    /// m  = app!().get_matches_from(vec![
    ///         "prog"
    ///     ]);
    ///
    /// assert_eq!(m.value_of("color"), Some("auto"));
    /// assert!(m.is_present("color"));
    /// assert_eq!(m.occurrences_of("color"), 0);
    ///
    /// // next, we'll provide a runtime value to override the default (as usually done).
    ///
    /// m  = app!().get_matches_from(vec![
    ///         "prog", "--color=never"
    ///     ]);
    ///
    /// assert_eq!(m.value_of("color"), Some("never"));
    /// assert!(m.is_present("color"));
    /// assert_eq!(m.occurrences_of("color"), 1);
    ///
    /// // finally, we will use the shortcut and only provide the argument without a value.
    ///
    /// m  = app!().get_matches_from(vec![
    ///         "prog", "--color"
    ///     ]);
    ///
    /// assert_eq!(m.value_of("color"), Some("always"));
    /// assert!(m.is_present("color"));
    /// assert_eq!(m.occurrences_of("color"), 1);
    /// ```
    /// [`ArgMatches::occurrences_of`]: ArgMatches::occurrences_of()
    /// [`ArgMatches::value_of`]: ArgMatches::value_of()
    /// [`Arg::takes_value(true)`]: Arg::takes_value()
    /// [`ArgMatches::is_present`]: ArgMatches::is_present()
    /// [`Arg::default_value`]: Arg::default_value()
    #[inline]
    pub fn default_missing_value(self, val: &'help str) -> Self {
        self.default_missing_values_os(&[OsStr::new(val)])
    }

    /// Provides a default value in the exact same manner as [`Arg::default_missing_value`]
    /// only using [`OsStr`]s instead.
    ///
    /// [`Arg::default_missing_value`]: Arg::default_missing_value()
    /// [`OsStr`]: std::ffi::OsStr
    #[inline]
    pub fn default_missing_value_os(self, val: &'help OsStr) -> Self {
        self.default_missing_values_os(&[val])
    }

    /// Like [`Arg::default_missing_value`] but for args taking multiple values
    ///
    /// [`Arg::default_missing_value`]: Arg::default_missing_value()
    #[inline]
    pub fn default_missing_values(self, vals: &[&'help str]) -> Self {
        let vals_vec: Vec<_> = vals.iter().map(|val| OsStr::new(*val)).collect();
        self.default_missing_values_os(&vals_vec[..])
    }

    /// Provides default values in the exact same manner as [`Arg::default_missing_values`]
    /// only using [`OsStr`]s instead.
    ///
    /// [`Arg::default_missing_values`]: Arg::default_missing_values()
    /// [`OsStr`]: std::ffi::OsStr
    #[inline]
    pub fn default_missing_values_os(mut self, vals: &[&'help OsStr]) -> Self {
        self.default_missing_vals = vals.to_vec();
        self.takes_value(true)
    }

    /// Specifies the value of the argument if `arg` has been used at runtime. If `val` is set to
    /// `None`, `arg` only needs to be present. If `val` is set to `"some-val"` then `arg` must be
    /// present at runtime **and** have the value `val`.
    ///
    /// **NOTE:** This setting is perfectly compatible with [`Arg::default_value`] but slightly
    /// different. `Arg::default_value` *only* takes effect when the user has not provided this arg
    /// at runtime. This setting however only takes effect when the user has not provided a value at
    /// runtime **and** these other conditions are met as well. If you have set `Arg::default_value`
    /// and `Arg::default_value_if`, and the user **did not** provide this arg at runtime, nor were
    /// the conditions met for `Arg::default_value_if`, the `Arg::default_value` will be applied.
    ///
    /// **NOTE:** This implicitly sets [`Arg::takes_value(true)`].
    ///
    /// # Examples
    ///
    /// First we use the default value only if another arg is present at runtime.
    ///
    /// ```rust
    /// # use clap::{App, Arg};
    /// let m = App::new("prog")
    ///     .arg(Arg::new("flag")
    ///         .long("flag"))
    ///     .arg(Arg::new("other")
    ///         .long("other")
    ///         .default_value_if("flag", None, Some("default")))
    ///     .get_matches_from(vec![
    ///         "prog", "--flag"
    ///     ]);
    ///
    /// assert_eq!(m.value_of("other"), Some("default"));
    /// ```
    ///
    /// If using YAML the values should be laid out as follows (`None` can be represented
    /// as `null` in YAML)
    ///
    /// ```yaml
    /// default_value_if:
    ///     - [arg, val, default]
    /// ```
    ///
    /// Next we run the same test, but without providing `--flag`.
    ///
    /// ```rust
    /// # use clap::{App, Arg};
    /// let m = App::new("prog")
    ///     .arg(Arg::new("flag")
    ///         .long("flag"))
    ///     .arg(Arg::new("other")
    ///         .long("other")
    ///         .default_value_if("flag", None, Some("default")))
    ///     .get_matches_from(vec![
    ///         "prog"
    ///     ]);
    ///
    /// assert_eq!(m.value_of("other"), None);
    /// ```
    ///
    /// Now lets only use the default value if `--opt` contains the value `special`.
    ///
    /// ```rust
    /// # use clap::{App, Arg};
    /// let m = App::new("prog")
    ///     .arg(Arg::new("opt")
    ///         .takes_value(true)
    ///         .long("opt"))
    ///     .arg(Arg::new("other")
    ///         .long("other")
    ///         .default_value_if("opt", Some("special"), Some("default")))
    ///     .get_matches_from(vec![
    ///         "prog", "--opt", "special"
    ///     ]);
    ///
    /// assert_eq!(m.value_of("other"), Some("default"));
    /// ```
    ///
    /// We can run the same test and provide any value *other than* `special` and we won't get a
    /// default value.
    ///
    /// ```rust
    /// # use clap::{App, Arg};
    /// let m = App::new("prog")
    ///     .arg(Arg::new("opt")
    ///         .takes_value(true)
    ///         .long("opt"))
    ///     .arg(Arg::new("other")
    ///         .long("other")
    ///         .default_value_if("opt", Some("special"), Some("default")))
    ///     .get_matches_from(vec![
    ///         "prog", "--opt", "hahaha"
    ///     ]);
    ///
    /// assert_eq!(m.value_of("other"), None);
    /// ```
    ///
    /// If we want to unset the default value for an Arg based on the presence or
    /// value of some other Arg.
    ///
    /// ```rust
    /// # use clap::{App, Arg};
    /// let m = App::new("prog")
    ///     .arg(Arg::new("flag")
    ///         .long("flag"))
    ///     .arg(Arg::new("other")
    ///         .long("other")
    ///         .default_value("default")
    ///         .default_value_if("flag", None, None))
    ///     .get_matches_from(vec![
    ///         "prog", "--flag"
    ///     ]);
    ///
    /// assert_eq!(m.value_of("other"), None);
    /// ```
    /// [`Arg::takes_value(true)`]: Arg::takes_value()
    /// [`Arg::default_value`]: Arg::default_value()
    pub fn default_value_if<T: Key>(
        self,
        arg_id: T,
        val: Option<&'help str>,
        default: Option<&'help str>,
    ) -> Self {
        self.default_value_if_os(arg_id, val.map(OsStr::new), default.map(OsStr::new))
    }

    /// Provides a conditional default value in the exact same manner as [`Arg::default_value_if`]
    /// only using [`OsStr`]s instead.
    ///
    /// [`Arg::default_value_if`]: Arg::default_value_if()
    /// [`OsStr`]: std::ffi::OsStr
    pub fn default_value_if_os<T: Key>(
        mut self,
        arg_id: T,
        val: Option<&'help OsStr>,
        default: Option<&'help OsStr>,
    ) -> Self {
        self.default_vals_ifs.push((arg_id.into(), val, default));
        self.takes_value(true)
    }

    /// Specifies multiple values and conditions in the same manner as [`Arg::default_value_if`].
    /// The method takes a slice of tuples in the `(arg, Option<val>, default)` format.
    ///
    /// **NOTE**: The conditions are stored in order and evaluated in the same order. I.e. the first
    /// if multiple conditions are true, the first one found will be applied and the ultimate value.
    ///
    /// # Examples
    ///
    /// First we use the default value only if another arg is present at runtime.
    ///
    /// ```rust
    /// # use clap::{App, Arg};
    /// let m = App::new("prog")
    ///     .arg(Arg::new("flag")
    ///         .long("flag"))
    ///     .arg(Arg::new("opt")
    ///         .long("opt")
    ///         .takes_value(true))
    ///     .arg(Arg::new("other")
    ///         .long("other")
    ///         .default_value_ifs(&[
    ///             ("flag", None, Some("default")),
    ///             ("opt", Some("channal"), Some("chan")),
    ///         ]))
    ///     .get_matches_from(vec![
    ///         "prog", "--opt", "channal"
    ///     ]);
    ///
    /// assert_eq!(m.value_of("other"), Some("chan"));
    /// ```
    ///
    /// If using YAML the values should be laid out as follows
    ///
    /// ```yaml
    /// default_value_if:
    ///     - [arg, val, default]
    ///     - [arg2, null, default2]
    /// ```
    ///
    /// Next we run the same test, but without providing `--flag`.
    ///
    /// ```rust
    /// # use clap::{App, Arg};
    /// let m = App::new("prog")
    ///     .arg(Arg::new("flag")
    ///         .long("flag"))
    ///     .arg(Arg::new("other")
    ///         .long("other")
    ///         .default_value_ifs(&[
    ///             ("flag", None, Some("default")),
    ///             ("opt", Some("channal"), Some("chan")),
    ///         ]))
    ///     .get_matches_from(vec![
    ///         "prog"
    ///     ]);
    ///
    /// assert_eq!(m.value_of("other"), None);
    /// ```
    ///
    /// We can also see that these values are applied in order, and if more than one condition is
    /// true, only the first evaluated "wins"
    ///
    /// ```rust
    /// # use clap::{App, Arg};
    /// let m = App::new("prog")
    ///     .arg(Arg::new("flag")
    ///         .long("flag"))
    ///     .arg(Arg::new("opt")
    ///         .long("opt")
    ///         .takes_value(true))
    ///     .arg(Arg::new("other")
    ///         .long("other")
    ///         .default_value_ifs(&[
    ///             ("flag", None, Some("default")),
    ///             ("opt", Some("channal"), Some("chan")),
    ///         ]))
    ///     .get_matches_from(vec![
    ///         "prog", "--opt", "channal", "--flag"
    ///     ]);
    ///
    /// assert_eq!(m.value_of("other"), Some("default"));
    /// ```
    /// [`Arg::takes_value(true)`]: Arg::takes_value()
    /// [`Arg::default_value_if`]: Arg::default_value_if()
    pub fn default_value_ifs<T: Key>(
        mut self,
        ifs: &[(T, Option<&'help str>, Option<&'help str>)],
    ) -> Self {
        for (arg, val, default) in ifs {
            self = self.default_value_if_os(arg, val.map(OsStr::new), default.map(OsStr::new));
        }
        self
    }

    /// Provides multiple conditional default values in the exact same manner as
    /// [`Arg::default_value_ifs`] only using [`OsStr`]s instead.
    ///
    /// [`Arg::default_value_ifs`]: Arg::default_value_ifs()
    /// [`OsStr`]: std::ffi::OsStr
    pub fn default_value_ifs_os<T: Key>(
        mut self,
        ifs: &[(T, Option<&'help OsStr>, Option<&'help OsStr>)],
    ) -> Self {
        for (arg, val, default) in ifs {
            self = self.default_value_if_os(arg.key(), *val, *default);
        }
        self
    }

    /// Specifies that if the value is not passed in as an argument, that it should be retrieved
    /// from the environment, if available. If it is not present in the environment, then default
    /// rules will apply.
    ///
    /// If user sets the argument in the environment:
    /// - When [`Arg::takes_value(true)`] is not set, the flag is considered raised.
    /// - When [`Arg::takes_value(true)`] is set, [`ArgMatches::value_of`] will
    ///   return value of the environment variable.
    ///
    /// If user doesn't set the argument in the environment:
    /// - When [`Arg::takes_value(true)`] is not set, the flag is considered off.
    /// - When [`Arg::takes_value(true)`] is set, [`ArgMatches::value_of`] will
    ///   return the default specified.
    ///
    /// # Examples
    ///
    /// In this example, we show the variable coming from the environment:
    ///
    /// ```rust
    /// # use std::env;
    /// # use clap::{App, Arg};
    ///
    /// env::set_var("MY_FLAG", "env");
    ///
    /// let m = App::new("prog")
    ///     .arg(Arg::new("flag")
    ///         .long("flag")
    ///         .env("MY_FLAG")
    ///         .takes_value(true))
    ///     .get_matches_from(vec![
    ///         "prog"
    ///     ]);
    ///
    /// assert_eq!(m.value_of("flag"), Some("env"));
    /// ```
    ///
    /// In this example, because [`Arg::takes_value(false)`] (by default),
    /// `prog` is a flag that accepts an optional, case-insensitive boolean literal.
    /// A `false` literal is `n`, `no`, `f`, `false`, `off` or `0`.
    /// An absent environment variable will also be considered as `false`.
    /// Anything else will considered as `true`.
    ///
    /// ```rust
    /// # use std::env;
    /// # use clap::{App, Arg};
    ///
    /// env::set_var("TRUE_FLAG", "true");
    /// env::set_var("FALSE_FLAG", "0");
    ///
    /// let m = App::new("prog")
    ///     .arg(Arg::new("true_flag")
    ///         .long("true_flag")
    ///         .env("TRUE_FLAG"))
    ///     .arg(Arg::new("false_flag")
    ///         .long("false_flag")
    ///         .env("FALSE_FLAG"))
    ///     .arg(Arg::new("absent_flag")
    ///         .long("absent_flag")
    ///         .env("ABSENT_FLAG"))
    ///     .get_matches_from(vec![
    ///         "prog"
    ///     ]);
    ///
    /// assert!(m.is_present("true_flag"));
    /// assert_eq!(m.value_of("true_flag"), None);
    /// assert!(!m.is_present("false_flag"));
    /// assert!(!m.is_present("absent_flag"));
    /// ```
    ///
    /// In this example, we show the variable coming from an option on the CLI:
    ///
    /// ```rust
    /// # use std::env;
    /// # use clap::{App, Arg};
    ///
    /// env::set_var("MY_FLAG", "env");
    ///
    /// let m = App::new("prog")
    ///     .arg(Arg::new("flag")
    ///         .long("flag")
    ///         .env("MY_FLAG")
    ///         .takes_value(true))
    ///     .get_matches_from(vec![
    ///         "prog", "--flag", "opt"
    ///     ]);
    ///
    /// assert_eq!(m.value_of("flag"), Some("opt"));
    /// ```
    ///
    /// In this example, we show the variable coming from the environment even with the
    /// presence of a default:
    ///
    /// ```rust
    /// # use std::env;
    /// # use clap::{App, Arg};
    ///
    /// env::set_var("MY_FLAG", "env");
    ///
    /// let m = App::new("prog")
    ///     .arg(Arg::new("flag")
    ///         .long("flag")
    ///         .env("MY_FLAG")
    ///         .takes_value(true)
    ///         .default_value("default"))
    ///     .get_matches_from(vec![
    ///         "prog"
    ///     ]);
    ///
    /// assert_eq!(m.value_of("flag"), Some("env"));
    /// ```
    ///
    /// In this example, we show the use of multiple values in a single environment variable:
    ///
    /// ```rust
    /// # use std::env;
    /// # use clap::{App, Arg};
    ///
    /// env::set_var("MY_FLAG_MULTI", "env1,env2");
    ///
    /// let m = App::new("prog")
    ///     .arg(Arg::new("flag")
    ///         .long("flag")
    ///         .env("MY_FLAG_MULTI")
    ///         .takes_value(true)
    ///         .multiple_values(true)
    ///         .use_delimiter(true))
    ///     .get_matches_from(vec![
    ///         "prog"
    ///     ]);
    ///
    /// assert_eq!(m.values_of("flag").unwrap().collect::<Vec<_>>(), vec!["env1", "env2"]);
    /// ```
    /// [`ArgMatches::occurrences_of`]: ArgMatches::occurrences_of()
    /// [`ArgMatches::value_of`]: crate::ArgMatches::value_of()
    /// [`ArgMatches::is_present`]: ArgMatches::is_present()
    /// [`Arg::takes_value(true)`]: Arg::takes_value()
    /// [`Arg::use_delimiter(true)`]: Arg::use_delimiter()
    #[cfg(feature = "env")]
    #[inline]
    pub fn env(self, name: &'help str) -> Self {
        self.env_os(OsStr::new(name))
    }

    /// Specifies that if the value is not passed in as an argument, that it should be retrieved
    /// from the environment if available in the exact same manner as [`Arg::env`] only using
    /// [`OsStr`]s instead.
    #[cfg(feature = "env")]
    #[inline]
    pub fn env_os(mut self, name: &'help OsStr) -> Self {
        self.env = Some((name, env::var_os(name)));
        self
    }

    /// Allows custom ordering of args within the help message. Args with a lower value will be
    /// displayed first in the help message. This is helpful when one would like to emphasise
    /// frequently used args, or prioritize those towards the top of the list. Duplicate values
    /// **are** allowed. Args with duplicate display orders will be displayed in alphabetical
    /// order.
    ///
    /// **NOTE:** The default is 999 for all arguments.
    ///
    /// **NOTE:** This setting is ignored for [positional arguments] which are always displayed in
    /// [index] order.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use clap::{App, Arg};
    /// let m = App::new("prog")
    ///     .arg(Arg::new("a") // Typically args are grouped alphabetically by name.
    ///                              // Args without a display_order have a value of 999 and are
    ///                              // displayed alphabetically with all other 999 valued args.
    ///         .long("long-option")
    ///         .short('o')
    ///         .takes_value(true)
    ///         .about("Some help and text"))
    ///     .arg(Arg::new("b")
    ///         .long("other-option")
    ///         .short('O')
    ///         .takes_value(true)
    ///         .display_order(1)   // In order to force this arg to appear *first*
    ///                             // all we have to do is give it a value lower than 999.
    ///                             // Any other args with a value of 1 will be displayed
    ///                             // alphabetically with this one...then 2 values, then 3, etc.
    ///         .about("I should be first!"))
    ///     .get_matches_from(vec![
    ///         "prog", "--help"
    ///     ]);
    /// ```
    ///
    /// The above example displays the following help message
    ///
    /// ```text
    /// cust-ord
    ///
    /// USAGE:
    ///     cust-ord [OPTIONS]
    ///
    /// OPTIONS:
    ///     -h, --help                Print help information
    ///     -V, --version             Print version information
    ///     -O, --other-option <b>    I should be first!
    ///     -o, --long-option <a>     Some help and text
    /// ```
    /// [positional arguments]: Arg::index()
    /// [index]: Arg::index()
    #[inline]
    pub fn display_order(mut self, ord: usize) -> Self {
        self.disp_ord = ord;
        self
    }

    /// Specifies that this arg is the last, or final, positional argument (i.e. has the highest
    /// index) and is *only* able to be accessed via the `--` syntax (i.e. `$ prog args --
    /// last_arg`). Even, if no other arguments are left to parse, if the user omits the `--` syntax
    /// they will receive an [`UnknownArgument`] error. Setting an argument to `.last(true)` also
    /// allows one to access this arg early using the `--` syntax. Accessing an arg early, even with
    /// the `--` syntax is otherwise not possible.
    ///
    /// **NOTE:** This will change the usage string to look like `$ prog [OPTIONS] [-- <ARG>]` if
    /// `ARG` is marked as `.last(true)`.
    ///
    /// **NOTE:** This setting will imply [`crate::AppSettings::DontCollapseArgsInUsage`] because failing
    /// to set this can make the usage string very confusing.
    ///
    /// **NOTE**: This setting only applies to positional arguments, and has no effect on OPTIONS
    ///
    /// **NOTE:** Setting this requires [`crate::ArgSettings::TakesValue`]
    ///
    /// **CAUTION:** Using this setting *and* having child subcommands is not
    /// recommended with the exception of *also* using [`crate::AppSettings::ArgsNegateSubcommands`]
    /// (or [`crate::AppSettings::SubcommandsNegateReqs`] if the argument marked `Last` is also
    /// marked [`ArgSettings::Required`])
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use clap::{Arg, ArgSettings};
    /// Arg::new("args")
    ///     .setting(ArgSettings::TakesValue)
    ///     .setting(ArgSettings::Last)
    /// # ;
    /// ```
    ///
    /// Setting [`ArgSettings::Last`] ensures the arg has the highest [index] of all positional args
    /// and requires that the `--` syntax be used to access it early.
    ///
    /// ```rust
    /// # use clap::{App, Arg, ArgSettings};
    /// let res = App::new("prog")
    ///     .arg(Arg::new("first"))
    ///     .arg(Arg::new("second"))
    ///     .arg(Arg::new("third")
    ///         .setting(ArgSettings::TakesValue)
    ///         .setting(ArgSettings::Last))
    ///     .try_get_matches_from(vec![
    ///         "prog", "one", "--", "three"
    ///     ]);
    ///
    /// assert!(res.is_ok());
    /// let m = res.unwrap();
    /// assert_eq!(m.value_of("third"), Some("three"));
    /// assert!(m.value_of("second").is_none());
    /// ```
    ///
    /// Even if the positional argument marked `Last` is the only argument left to parse,
    /// failing to use the `--` syntax results in an error.
    ///
    /// ```rust
    /// # use clap::{App, Arg, ErrorKind, ArgSettings};
    /// let res = App::new("prog")
    ///     .arg(Arg::new("first"))
    ///     .arg(Arg::new("second"))
    ///     .arg(Arg::new("third")
    ///         .setting(ArgSettings::TakesValue)
    ///         .setting(ArgSettings::Last))
    ///     .try_get_matches_from(vec![
    ///         "prog", "one", "two", "three"
    ///     ]);
    ///
    /// assert!(res.is_err());
    /// assert_eq!(res.unwrap_err().kind, ErrorKind::UnknownArgument);
    /// ```
    /// [index]: Arg::index()
    /// [`UnknownArgument`]: crate::ErrorKind::UnknownArgument
    #[inline]
    pub fn last(self, l: bool) -> Self {
        if l {
            self.setting(ArgSettings::Last)
        } else {
            self.unset_setting(ArgSettings::Last)
        }
    }

    /// Specifies that the argument is required by default. Required by default means it is
    /// required, when no other conflicting rules or overrides have been evaluated. Conflicting
    /// rules take precedence over being required.
    ///
    /// **NOTE:** The default is `false`.
    ///
    /// **Pro tip:** Flags (i.e. not positional, or arguments that take values) shouldn't be
    /// required by default. This is because if a flag were to be required, it should simply be
    /// implied. No additional information is required from user. Flags by their very nature are
    /// simply boolean on/off switches. The only time a user *should* be required to use a flag
    /// is if the operation is destructive in nature, and the user is essentially proving to you,
    /// "Yes, I know what I'm doing."
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use clap::{Arg, ArgSettings};
    /// Arg::new("config")
    ///     .required(true)  // equivalent to .setting(ArgSettings::Required)
    /// # ;
    /// ```
    ///
    /// Setting [`Required`] requires that the argument be used at runtime.
    ///
    /// ```rust
    /// # use clap::{App, Arg, ArgSettings};
    /// let res = App::new("prog")
    ///     .arg(Arg::new("cfg")
    ///         .setting(ArgSettings::Required)
    ///         .setting(ArgSettings::TakesValue)
    ///         .long("config"))
    ///     .try_get_matches_from(vec![
    ///         "prog", "--config", "file.conf",
    ///     ]);
    ///
    /// assert!(res.is_ok());
    /// ```
    ///
    /// Setting [`Required`] and then *not* supplying that argument at runtime is an error.
    ///
    /// ```rust
    /// # use clap::{App, Arg, ArgSettings, ErrorKind};
    /// let res = App::new("prog")
    ///     .arg(Arg::new("cfg")
    ///         .setting(ArgSettings::Required)
    ///         .setting(ArgSettings::TakesValue)
    ///         .long("config"))
    ///     .try_get_matches_from(vec![
    ///         "prog"
    ///     ]);
    ///
    /// assert!(res.is_err());
    /// assert_eq!(res.unwrap_err().kind, ErrorKind::MissingRequiredArgument);
    /// ```
    /// [`Required`]: ArgSettings::Required
    #[inline]
    pub fn required(self, r: bool) -> Self {
        if r {
            self.setting(ArgSettings::Required)
        } else {
            self.unset_setting(ArgSettings::Required)
        }
    }

    /// Specifies that the argument takes a value at run time.
    ///
    /// **NOTE:** values for arguments may be specified in any of the following methods
    ///
    /// * Using a space such as `-o value` or `--option value`
    /// * Using an equals and no space such as `-o=value` or `--option=value`
    /// * Use a short and no space such as `-ovalue`
    ///
    /// **NOTE:** By default, args which allow [multiple values] are delimited by commas, meaning
    /// `--option=val1,val2,val3` is three values for the `--option` argument. If you wish to
    /// change the delimiter to another character you can use [`Arg::value_delimiter(char)`],
    /// alternatively you can turn delimiting values **OFF** by using
    /// [`Arg::unset_setting(ArgSettings::UseValueDelimiter)`]
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use clap::{App, Arg, ArgSettings};
    /// Arg::new("config")
    ///     .setting(ArgSettings::TakesValue)
    /// # ;
    /// ```
    ///
    /// ```rust
    /// # use clap::{App, Arg, ArgSettings};
    /// let m = App::new("prog")
    ///     .arg(Arg::new("mode")
    ///         .long("mode")
    ///         .setting(ArgSettings::TakesValue))
    ///     .get_matches_from(vec![
    ///         "prog", "--mode", "fast"
    ///     ]);
    ///
    /// assert!(m.is_present("mode"));
    /// assert_eq!(m.value_of("mode"), Some("fast"));
    /// ```
    /// [`Arg::value_delimiter(char)`]: Arg::value_delimiter()
    /// [`Arg::unset_setting(ArgSettings::UseValueDelimiter)`]: ArgSettings::UseValueDelimiter
    /// [multiple values]: ArgSettings::MultipleValues
    #[inline]
    pub fn takes_value(self, tv: bool) -> Self {
        if tv {
            self.setting(ArgSettings::TakesValue)
        } else {
            self.unset_setting(ArgSettings::TakesValue)
        }
    }

    /// Allows values which start with a leading hyphen (`-`)
    ///
    /// **NOTE:** Setting this requires [`ArgSettings::TakesValue`]
    ///
    /// **WARNING**: Take caution when using this setting combined with
    /// [`ArgSettings::MultipleValues`], as this becomes ambiguous `$ prog --arg -- -- val`. All
    /// three `--, --, val` will be values when the user may have thought the second `--` would
    /// constitute the normal, "Only positional args follow" idiom. To fix this, consider using
    /// [`ArgSettings::MultipleOccurrences`] which only allows a single value at a time.
    ///
    /// **WARNING**: When building your CLIs, consider the effects of allowing leading hyphens and
    /// the user passing in a value that matches a valid short. For example, `prog -opt -F` where
    /// `-F` is supposed to be a value, yet `-F` is *also* a valid short for another arg.
    /// Care should be taken when designing these args. This is compounded by the ability to "stack"
    /// short args. I.e. if `-val` is supposed to be a value, but `-v`, `-a`, and `-l` are all valid
    /// shorts.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use clap::{Arg, ArgSettings};
    /// Arg::new("pattern")
    ///     .setting(ArgSettings::TakesValue)
    ///     .setting(ArgSettings::AllowHyphenValues)
    /// # ;
    /// ```
    ///
    /// ```rust
    /// # use clap::{App, Arg, ArgSettings};
    /// let m = App::new("prog")
    ///     .arg(Arg::new("pat")
    ///         .setting(ArgSettings::TakesValue)
    ///         .setting(ArgSettings::AllowHyphenValues)
    ///         .long("pattern"))
    ///     .get_matches_from(vec![
    ///         "prog", "--pattern", "-file"
    ///     ]);
    ///
    /// assert_eq!(m.value_of("pat"), Some("-file"));
    /// ```
    ///
    /// Not setting `Arg::allow_hyphen_values(true)` and supplying a value which starts with a
    /// hyphen is an error.
    ///
    /// ```rust
    /// # use clap::{App, Arg, ErrorKind, ArgSettings};
    /// let res = App::new("prog")
    ///     .arg(Arg::new("pat")
    ///         .setting(ArgSettings::TakesValue)
    ///         .long("pattern"))
    ///     .try_get_matches_from(vec![
    ///         "prog", "--pattern", "-file"
    ///     ]);
    ///
    /// assert!(res.is_err());
    /// assert_eq!(res.unwrap_err().kind, ErrorKind::UnknownArgument);
    /// ```
    /// [`Arg::number_of_values(1)`]: Arg::number_of_values()
    #[inline]
    pub fn allow_hyphen_values(self, a: bool) -> Self {
        if a {
            self.setting(ArgSettings::AllowHyphenValues)
        } else {
            self.unset_setting(ArgSettings::AllowHyphenValues)
        }
    }

    /// Requires that options use the `--option=val` syntax (i.e. an equals between the option and
    /// associated value) **Default:** `false`
    ///
    /// **NOTE:** Setting this requires [`ArgSettings::TakesValue`]
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use clap::{Arg, ArgSettings};
    /// Arg::new("config")
    ///     .long("config")
    ///     .setting(ArgSettings::TakesValue)
    ///     .setting(ArgSettings::RequireEquals)
    /// # ;
    /// ```
    ///
    /// Setting [`RequireEquals`] requires that the option have an equals sign between
    /// it and the associated value.
    ///
    /// ```rust
    /// # use clap::{App, Arg, ArgSettings};
    /// let res = App::new("prog")
    ///     .arg(Arg::new("cfg")
    ///         .setting(ArgSettings::TakesValue)
    ///         .setting(ArgSettings::RequireEquals)
    ///         .long("config"))
    ///     .try_get_matches_from(vec![
    ///         "prog", "--config=file.conf"
    ///     ]);
    ///
    /// assert!(res.is_ok());
    /// ```
    ///
    /// Setting [`RequireEquals`] and *not* supplying the equals will cause an
    /// error.
    ///
    /// ```rust
    /// # use clap::{App, Arg, ErrorKind, ArgSettings};
    /// let res = App::new("prog")
    ///     .arg(Arg::new("cfg")
    ///         .setting(ArgSettings::TakesValue)
    ///         .setting(ArgSettings::RequireEquals)
    ///         .long("config"))
    ///     .try_get_matches_from(vec![
    ///         "prog", "--config", "file.conf"
    ///     ]);
    ///
    /// assert!(res.is_err());
    /// assert_eq!(res.unwrap_err().kind, ErrorKind::NoEquals);
    /// ```
    /// [`RequireEquals`]: ArgSettings::RequireEquals
    #[inline]
    pub fn require_equals(self, r: bool) -> Self {
        if r {
            self.setting(ArgSettings::RequireEquals)
        } else {
            self.unset_setting(ArgSettings::RequireEquals)
        }
    }

    /// Specifies that an argument can be matched to all child [`Subcommand`]s.
    ///
    /// **NOTE:** Global arguments *only* propagate down, **not** up (to parent commands), however
    /// their values once a user uses them will be propagated back up to parents. In effect, this
    /// means one should *define* all global arguments at the top level, however it doesn't matter
    /// where the user *uses* the global argument.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use clap::{App, Arg, ArgSettings};
    /// Arg::new("debug")
    ///     .short('d')
    ///     .global(true)
    /// # ;
    /// ```
    ///
    /// For example, assume an application with two subcommands, and you'd like to define a
    /// `--verbose` flag that can be called on any of the subcommands and parent, but you don't
    /// want to clutter the source with three duplicate [`Arg`] definitions.
    ///
    /// ```rust
    /// # use clap::{App, Arg, ArgSettings};
    /// let m = App::new("prog")
    ///     .arg(Arg::new("verb")
    ///         .long("verbose")
    ///         .short('v')
    ///         .global(true))
    ///     .subcommand(App::new("test"))
    ///     .subcommand(App::new("do-stuff"))
    ///     .get_matches_from(vec![
    ///         "prog", "do-stuff", "--verbose"
    ///     ]);
    ///
    /// assert_eq!(m.subcommand_name(), Some("do-stuff"));
    /// let sub_m = m.subcommand_matches("do-stuff").unwrap();
    /// assert!(sub_m.is_present("verb"));
    /// ```
    ///
    /// [`Subcommand`]: crate::Subcommand
    /// [required]: ArgSettings::Required
    /// [`ArgMatches::is_present("flag")`]: ArgMatches::is_present()
    #[inline]
    pub fn global(mut self, g: bool) -> Self {
        self.global = g;
        self
    }

    /// Specifies that *multiple values* may only be set using the delimiter. This means if an
    /// option is encountered, and no delimiter is found, it is assumed that no additional values
    /// for that option follow. This is unlike the default, where it is generally assumed that
    /// more values will follow regardless of whether or not a delimiter is used.
    ///
    /// **NOTE:** The default is `false`.
    ///
    /// **NOTE:** Setting this requires [`ArgSettings::UseValueDelimiter`] and
    /// [`ArgSettings::TakesValue`]
    ///
    /// **NOTE:** It's a good idea to inform the user that use of a delimiter is required, either
    /// through help text or other means.
    ///
    /// # Examples
    ///
    /// These examples demonstrate what happens when `require_delimiter(true)` is used. Notice
    /// everything works in this first example, as we use a delimiter, as expected.
    ///
    /// ```rust
    /// # use clap::{App, Arg, ArgSettings};
    /// let delims = App::new("prog")
    ///     .arg(Arg::new("opt")
    ///         .short('o')
    ///         .setting(ArgSettings::TakesValue)
    ///         .setting(ArgSettings::UseValueDelimiter)
    ///         .setting(ArgSettings::RequireDelimiter)
    ///         .setting(ArgSettings::MultipleValues))
    ///     .get_matches_from(vec![
    ///         "prog", "-o", "val1,val2,val3",
    ///     ]);
    ///
    /// assert!(delims.is_present("opt"));
    /// assert_eq!(delims.values_of("opt").unwrap().collect::<Vec<_>>(), ["val1", "val2", "val3"]);
    /// ```
    ///
    /// In this next example, we will *not* use a delimiter. Notice it's now an error.
    ///
    /// ```rust
    /// # use clap::{App, Arg, ErrorKind, ArgSettings};
    /// let res = App::new("prog")
    ///     .arg(Arg::new("opt")
    ///         .short('o')
    ///         .setting(ArgSettings::TakesValue)
    ///         .setting(ArgSettings::UseValueDelimiter)
    ///         .setting(ArgSettings::RequireDelimiter))
    ///     .try_get_matches_from(vec![
    ///         "prog", "-o", "val1", "val2", "val3",
    ///     ]);
    ///
    /// assert!(res.is_err());
    /// let err = res.unwrap_err();
    /// assert_eq!(err.kind, ErrorKind::UnknownArgument);
    /// ```
    ///
    /// What's happening is `-o` is getting `val1`, and because delimiters are required yet none
    /// were present, it stops parsing `-o`. At this point it reaches `val2` and because no
    /// positional arguments have been defined, it's an error of an unexpected argument.
    ///
    /// In this final example, we contrast the above with `clap`'s default behavior where the above
    /// is *not* an error.
    ///
    /// ```rust
    /// # use clap::{App, Arg, ArgSettings};
    /// let delims = App::new("prog")
    ///     .arg(Arg::new("opt")
    ///         .short('o')
    ///         .setting(ArgSettings::TakesValue)
    ///         .setting(ArgSettings::MultipleValues))
    ///     .get_matches_from(vec![
    ///         "prog", "-o", "val1", "val2", "val3",
    ///     ]);
    ///
    /// assert!(delims.is_present("opt"));
    /// assert_eq!(delims.values_of("opt").unwrap().collect::<Vec<_>>(), ["val1", "val2", "val3"]);
    /// ```
    #[inline]
    pub fn require_delimiter(self, d: bool) -> Self {
        if d {
            self.setting(ArgSettings::RequireDelimiter)
        } else {
            self.unset_setting(ArgSettings::RequireDelimiter)
        }
    }

    /// Specifies if the possible values of an argument should be displayed in the help text or
    /// not. Defaults to `false` (i.e. show possible values)
    ///
    /// This is useful for args with many values, or ones which are explained elsewhere in the
    /// help text.
    ///
    /// **NOTE:** Setting this requires [`ArgSettings::TakesValue`]
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use clap::{App, Arg, ArgSettings};
    /// Arg::new("config")
    ///     .setting(ArgSettings::TakesValue)
    ///     .setting(ArgSettings::HidePossibleValues)
    /// # ;
    /// ```
    ///
    /// ```rust
    /// # use clap::{App, Arg, ArgSettings};
    /// let m = App::new("prog")
    ///     .arg(Arg::new("mode")
    ///         .long("mode")
    ///         .possible_values(["fast", "slow"])
    ///         .setting(ArgSettings::TakesValue)
    ///         .setting(ArgSettings::HidePossibleValues));
    /// ```
    /// If we were to run the above program with `--help` the `[values: fast, slow]` portion of
    /// the help text would be omitted.
    #[inline]
    pub fn hide_possible_values(self, hide: bool) -> Self {
        if hide {
            self.setting(ArgSettings::HidePossibleValues)
        } else {
            self.unset_setting(ArgSettings::HidePossibleValues)
        }
    }

    /// Specifies that the default value of an argument should not be displayed in the help text.
    ///
    /// This is useful when default behavior of an arg is explained elsewhere in the help text.
    ///
    /// **NOTE:** Setting this requires [`ArgSettings::TakesValue`]
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use clap::{App, Arg, ArgSettings};
    /// Arg::new("config")
    ///     .setting(ArgSettings::TakesValue)
    ///     .setting(ArgSettings::HideDefaultValue)
    /// # ;
    /// ```
    ///
    /// ```rust
    /// # use clap::{App, Arg, ArgSettings};
    /// let m = App::new("connect")
    ///     .arg(Arg::new("host")
    ///         .long("host")
    ///         .default_value("localhost")
    ///         .setting(ArgSettings::TakesValue)
    ///         .setting(ArgSettings::HideDefaultValue));
    ///
    /// ```
    ///
    /// If we were to run the above program with `--help` the `[default: localhost]` portion of
    /// the help text would be omitted.
    #[inline]
    pub fn hide_default_value(self, hide: bool) -> Self {
        if hide {
            self.setting(ArgSettings::HideDefaultValue)
        } else {
            self.unset_setting(ArgSettings::HideDefaultValue)
        }
    }

    /// Hides an argument from help message output.
    ///
    /// **NOTE:** This does **not** hide the argument from usage strings on error
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use clap::{App, Arg, ArgSettings};
    /// Arg::new("debug")
    ///     .setting(ArgSettings::Hidden)
    /// # ;
    /// ```
    /// Setting `Hidden` will hide the argument when displaying help text
    ///
    /// ```rust
    /// # use clap::{App, Arg, ArgSettings};
    /// let m = App::new("prog")
    ///     .arg(Arg::new("cfg")
    ///         .long("config")
    ///         .setting(ArgSettings::Hidden)
    ///         .about("Some help text describing the --config arg"))
    ///     .get_matches_from(vec![
    ///         "prog", "--help"
    ///     ]);
    /// ```
    ///
    /// The above example displays
    ///
    /// ```text
    /// helptest
    ///
    /// USAGE:
    ///    helptest [OPTIONS]
    ///
    /// OPTIONS:
    /// -h, --help       Print help information
    /// -V, --version    Print version information
    /// ```
    #[inline]
    pub fn hidden(self, h: bool) -> Self {
        if h {
            self.setting(ArgSettings::Hidden)
        } else {
            self.unset_setting(ArgSettings::Hidden)
        }
    }

    /// When used with [`Arg::possible_values`] it allows the argument
    /// value to pass validation even if the case differs from that of
    /// the specified `possible_value`.
    ///
    /// When other arguments are conditionally required based on the
    /// value of a case-insensitive argument, the equality check done
    /// by [`Arg::required_if_eq`], [`Arg::required_if_eq_any`], or
    /// [`Arg::required_if_eq_all`] is case-insensitive.
    ///
    ///
    /// **NOTE:** Setting this requires [`ArgSettings::TakesValue`]
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use clap::{App, Arg, ArgSettings};
    /// let m = App::new("pv")
    ///     .arg(Arg::new("option")
    ///         .long("--option")
    ///         .setting(ArgSettings::TakesValue)
    ///         .setting(ArgSettings::IgnoreCase)
    ///         .possible_value("test123"))
    ///     .get_matches_from(vec![
    ///         "pv", "--option", "TeSt123",
    ///     ]);
    ///
    /// assert!(m.value_of("option").unwrap().eq_ignore_ascii_case("test123"));
    /// ```
    ///
    /// This setting also works when multiple values can be defined:
    ///
    /// ```rust
    /// # use clap::{App, Arg, ArgSettings};
    /// let m = App::new("pv")
    ///     .arg(Arg::new("option")
    ///         .short('o')
    ///         .long("--option")
    ///         .setting(ArgSettings::IgnoreCase)
    ///         .setting(ArgSettings::TakesValue)
    ///         .setting(ArgSettings::MultipleValues)
    ///         .possible_value("test123")
    ///         .possible_value("test321"))
    ///     .get_matches_from(vec![
    ///         "pv", "--option", "TeSt123", "teST123", "tESt321"
    ///     ]);
    ///
    /// let matched_vals = m.values_of("option").unwrap().collect::<Vec<_>>();
    /// assert_eq!(&*matched_vals, &["TeSt123", "teST123", "tESt321"]);
    /// ```
    #[inline]
    pub fn case_insensitive(self, ci: bool) -> Self {
        if ci {
            self.setting(ArgSettings::IgnoreCase)
        } else {
            self.unset_setting(ArgSettings::IgnoreCase)
        }
    }

    /// Specifies that an argument should allow grouping of multiple values via a
    /// delimiter. I.e. should `--option=val1,val2,val3` be parsed as three values (`val1`, `val2`,
    /// and `val3`) or as a single value (`val1,val2,val3`). Defaults to using `,` (comma) as the
    /// value delimiter for all arguments that accept values (options and positional arguments)
    ///
    /// **NOTE:** When this setting is used, it will default [`Arg::value_delimiter`]
    /// to the comma `,`.
    ///
    /// **NOTE:** Implicitly sets [`ArgSettings::TakesValue`]
    ///
    /// # Examples
    ///
    /// The following example shows the default behavior.
    ///
    /// ```rust
    /// # use clap::{App, Arg, ArgSettings};
    /// let delims = App::new("prog")
    ///     .arg(Arg::new("option")
    ///         .long("option")
    ///         .setting(ArgSettings::UseValueDelimiter)
    ///         .takes_value(true))
    ///     .get_matches_from(vec![
    ///         "prog", "--option=val1,val2,val3",
    ///     ]);
    ///
    /// assert!(delims.is_present("option"));
    /// assert_eq!(delims.occurrences_of("option"), 1);
    /// assert_eq!(delims.values_of("option").unwrap().collect::<Vec<_>>(), ["val1", "val2", "val3"]);
    /// ```
    /// The next example shows the difference when turning delimiters off. This is the default
    /// behavior
    ///
    /// ```rust
    /// # use clap::{App, Arg, ArgSettings};
    /// let nodelims = App::new("prog")
    ///     .arg(Arg::new("option")
    ///         .long("option")
    ///         .setting(ArgSettings::TakesValue))
    ///     .get_matches_from(vec![
    ///         "prog", "--option=val1,val2,val3",
    ///     ]);
    ///
    /// assert!(nodelims.is_present("option"));
    /// assert_eq!(nodelims.occurrences_of("option"), 1);
    /// assert_eq!(nodelims.value_of("option").unwrap(), "val1,val2,val3");
    /// ```
    /// [`Arg::value_delimiter`]: Arg::value_delimiter()
    #[inline]
    pub fn use_delimiter(mut self, d: bool) -> Self {
        if d {
            if self.val_delim.is_none() {
                self.val_delim = Some(',');
            }
            self.takes_value(true)
                .setting(ArgSettings::UseValueDelimiter)
        } else {
            self.val_delim = None;
            self.unset_setting(ArgSettings::UseValueDelimiter)
        }
    }

    /// Specifies that environment variable arguments should not be displayed in the help text.
    ///
    /// This is useful when the variable option is explained elsewhere in the help text.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use clap::{App, Arg, ArgSettings};
    /// Arg::new("config")
    ///     .setting(ArgSettings::TakesValue)
    ///     .setting(ArgSettings::HideEnv)
    /// # ;
    /// ```
    ///
    /// ```rust
    /// # use clap::{App, Arg, ArgSettings};
    /// let m = App::new("prog")
    ///     .arg(Arg::new("mode")
    ///         .long("mode")
    ///         .env("MODE")
    ///         .setting(ArgSettings::TakesValue)
    ///         .setting(ArgSettings::HideEnv));
    ///
    /// ```
    ///
    /// If we were to run the above program with `--help` the `[env: MODE]` portion of the help
    /// text would be omitted.
    #[cfg(feature = "env")]
    #[inline]
    pub fn hide_env(self, hide: bool) -> Self {
        if hide {
            self.setting(ArgSettings::HideEnv)
        } else {
            self.unset_setting(ArgSettings::HideEnv)
        }
    }

    /// Specifies that any values inside the associated ENV variables of an argument should not be
    /// displayed in the help text.
    ///
    /// This is useful when ENV vars contain sensitive values.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use clap::{App, Arg, ArgSettings};
    /// Arg::new("config")
    ///     .setting(ArgSettings::TakesValue)
    ///     .setting(ArgSettings::HideEnvValues)
    /// # ;
    /// ```
    ///
    /// ```rust
    /// # use clap::{App, Arg, ArgSettings};
    /// let m = App::new("connect")
    ///     .arg(Arg::new("host")
    ///         .long("host")
    ///         .env("CONNECT")
    ///         .setting(ArgSettings::TakesValue)
    ///         .setting(ArgSettings::HideEnvValues));
    ///
    /// ```
    ///
    /// If we were to run the above program with `$ CONNECT=super_secret connect --help` the
    /// `[default: CONNECT=super_secret]` portion of the help text would be omitted.
    #[cfg(feature = "env")]
    #[inline]
    pub fn hide_env_values(self, hide: bool) -> Self {
        if hide {
            self.setting(ArgSettings::HideEnvValues)
        } else {
            self.unset_setting(ArgSettings::HideEnvValues)
        }
    }

    /// When set to `true` the help string will be displayed on the line after the argument and
    /// indented once. This can be helpful for arguments with very long or complex help messages.
    /// This can also be helpful for arguments with very long flag names, or many/long value names.
    ///
    /// **NOTE:** To apply this setting to all arguments consider using
    /// [`crate::AppSettings::NextLineHelp`]
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use clap::{App, Arg, ArgSettings};
    /// let m = App::new("prog")
    ///     .arg(Arg::new("opt")
    ///         .long("long-option-flag")
    ///         .short('o')
    ///         .setting(ArgSettings::TakesValue)
    ///         .setting(ArgSettings::NextLineHelp)
    ///         .value_names(&["value1", "value2"])
    ///         .about("Some really long help and complex\n\
    ///                help that makes more sense to be\n\
    ///                on a line after the option"))
    ///     .get_matches_from(vec![
    ///         "prog", "--help"
    ///     ]);
    /// ```
    ///
    /// The above example displays the following help message
    ///
    /// ```text
    /// nlh
    ///
    /// USAGE:
    ///     nlh [OPTIONS]
    ///
    /// OPTIONS:
    ///     -h, --help       Print help information
    ///     -V, --version    Print version information
    ///     -o, --long-option-flag <value1> <value2>
    ///         Some really long help and complex
    ///         help that makes more sense to be
    ///         on a line after the option
    /// ```
    #[inline]
    pub fn next_line_help(self, nlh: bool) -> Self {
        if nlh {
            self.setting(ArgSettings::NextLineHelp)
        } else {
            self.unset_setting(ArgSettings::NextLineHelp)
        }
    }

    /// Don't allow an argument to accept explicitly empty values. An empty value
    /// must be specified at the command line with an explicit `""`, `''`, or
    /// `--option=`
    ///
    /// **NOTE:** By default empty values are allowed.
    ///
    /// **NOTE:** Setting this requires [`ArgSettings::TakesValue`].
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use clap::{App, Arg, ArgSettings};
    /// Arg::new("file")
    ///     .long("file")
    ///     .takes_value(true)
    ///     .forbid_empty_values(true)
    /// # ;
    /// ```
    ///
    /// The default is allowing empty values.
    ///
    /// ```rust
    /// # use clap::{App, Arg, ErrorKind, ArgSettings};
    /// let res = App::new("prog")
    ///     .arg(Arg::new("cfg")
    ///         .long("config")
    ///         .short('v')
    ///         .takes_value(true))
    ///     .try_get_matches_from(vec![
    ///         "prog", "--config="
    ///     ]);
    ///
    /// assert!(res.is_ok());
    /// assert_eq!(res.unwrap().value_of("cfg"), Some(""));
    /// ```
    ///
    /// By adding this setting, we can forbid empty values.
    ///
    /// ```rust
    /// # use clap::{App, Arg, ErrorKind, ArgSettings};
    /// let res = App::new("prog")
    ///     .arg(Arg::new("cfg")
    ///         .long("config")
    ///         .short('v')
    ///         .takes_value(true)
    ///         .forbid_empty_values(true))
    ///     .try_get_matches_from(vec![
    ///         "prog", "--config="
    ///     ]);
    ///
    /// assert!(res.is_err());
    /// assert_eq!(res.unwrap_err().kind, ErrorKind::EmptyValue);
    /// ```
    #[inline]
    pub fn forbid_empty_values(self, empty: bool) -> Self {
        if empty {
            self.setting(ArgSettings::ForbidEmptyValues)
        } else {
            self.unset_setting(ArgSettings::ForbidEmptyValues)
        }
    }

    /// Deprecated, see [`Arg::forbid_empty_values`]
    #[deprecated(since = "3.0.0", note = "Replaced with `Arg::forbid_empty_values`")]
    pub fn empty_values(self, empty: bool) -> Self {
        self.forbid_empty_values(!empty)
    }

    /// Specifies that the argument may have an unknown number of multiple values. Without any other
    /// settings, this argument may appear only *once*.
    ///
    /// For example, `--opt val1 val2` is allowed, but `--opt val1 val2 --opt val3` is not.
    ///
    /// **NOTE:** Setting this requires [`ArgSettings::TakesValue`].
    ///
    /// **WARNING:**
    ///
    /// Setting `MultipleValues` for an argument that takes a value, but with no other details can
    /// be dangerous in some circumstances. Because multiple values are allowed,
    /// `--option val1 val2 val3` is perfectly valid. Be careful when designing a CLI where
    /// positional arguments are *also* expected as `clap` will continue parsing *values* until one
    /// of the following happens:
    ///
    /// * It reaches the [maximum number of values]
    /// * It reaches a [specific number of values]
    /// * It finds another flag or option (i.e. something that starts with a `-`)
    ///
    /// **WARNING:**
    ///
    /// When using args with `MultipleValues` and [`subcommands`], one needs to consider the
    /// possibility of an argument value being the same as a valid subcommand. By default `clap` will
    /// parse the argument in question as a value *only if* a value is possible at that moment.
    /// Otherwise it will be parsed as a subcommand. In effect, this means using `MultipleValues` with no
    /// additional parameters and a value that coincides with a subcommand name, the subcommand
    /// cannot be called unless another argument is passed between them.
    ///
    /// As an example, consider a CLI with an option `--ui-paths=<paths>...` and subcommand `signer`
    ///
    /// The following would be parsed as values to `--ui-paths`.
    ///
    /// ```text
    /// $ program --ui-paths path1 path2 signer
    /// ```
    ///
    /// This is because `--ui-paths` accepts multiple values. `clap` will continue parsing values
    /// until another argument is reached and it knows `--ui-paths` is done parsing.
    ///
    /// By adding additional parameters to `--ui-paths` we can solve this issue. Consider adding
    /// [`Arg::number_of_values(1)`] or using *only* [`MultipleOccurrences`]. The following are all
    /// valid, and `signer` is parsed as a subcommand in the first case, but a value in the second
    /// case.
    ///
    /// ```text
    /// $ program --ui-paths path1 signer
    /// $ program --ui-paths path1 --ui-paths signer signer
    /// ```
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use clap::{App, Arg, ArgSettings};
    /// Arg::new("debug")
    ///     .short('d')
    ///     .setting(ArgSettings::TakesValue)
    ///     .setting(ArgSettings::MultipleValues);
    /// ```
    ///
    /// An example with options
    ///
    /// ```rust
    /// # use clap::{App, Arg, ArgSettings};
    /// let m = App::new("prog")
    ///     .arg(Arg::new("file")
    ///         .setting(ArgSettings::TakesValue)
    ///         .setting(ArgSettings::MultipleValues)
    ///         .short('F'))
    ///     .get_matches_from(vec![
    ///         "prog", "-F", "file1", "file2", "file3"
    ///     ]);
    ///
    /// assert!(m.is_present("file"));
    /// assert_eq!(m.occurrences_of("file"), 1); // notice only one occurrence
    /// let files: Vec<_> = m.values_of("file").unwrap().collect();
    /// assert_eq!(files, ["file1", "file2", "file3"]);
    /// ```
    ///
    /// Although `MultipleVlaues` has been specified, we cannot use the argument more than once.
    ///
    /// ```rust
    /// # use clap::{App, Arg, ErrorKind, ArgSettings};
    /// let res = App::new("prog")
    ///     .arg(Arg::new("file")
    ///         .setting(ArgSettings::TakesValue)
    ///         .setting(ArgSettings::MultipleValues)
    ///         .short('F'))
    ///     .try_get_matches_from(vec![
    ///         "prog", "-F", "file1", "-F", "file2", "-F", "file3"
    ///     ]);
    ///
    /// assert!(res.is_err());
    /// assert_eq!(res.unwrap_err().kind, ErrorKind::UnexpectedMultipleUsage)
    /// ```
    ///
    /// A common mistake is to define an option which allows multiple values, and a positional
    /// argument.
    ///
    /// ```rust
    /// # use clap::{App, Arg, ArgSettings};
    /// let m = App::new("prog")
    ///     .arg(Arg::new("file")
    ///         .setting(ArgSettings::TakesValue)
    ///         .setting(ArgSettings::MultipleValues)
    ///         .short('F'))
    ///     .arg(Arg::new("word")
    ///         .index(1))
    ///     .get_matches_from(vec![
    ///         "prog", "-F", "file1", "file2", "file3", "word"
    ///     ]);
    ///
    /// assert!(m.is_present("file"));
    /// let files: Vec<_> = m.values_of("file").unwrap().collect();
    /// assert_eq!(files, ["file1", "file2", "file3", "word"]); // wait...what?!
    /// assert!(!m.is_present("word")); // but we clearly used word!
    /// ```
    ///
    /// The problem is `clap` doesn't know when to stop parsing values for "files". This is further
    /// compounded by if we'd said `word -F file1 file2` it would have worked fine, so it would
    /// appear to only fail sometimes...not good!
    ///
    /// A solution for the example above is to limit how many values with a [maximum], or [specific]
    /// number, or to say [`MultipleOccurrences`] is ok, but multiple values is not.
    ///
    /// ```rust
    /// # use clap::{App, Arg, ArgSettings};
    /// let m = App::new("prog")
    ///     .arg(Arg::new("file")
    ///         .setting(ArgSettings::MultipleOccurrences)
    ///         .setting(ArgSettings::TakesValue)
    ///         .short('F'))
    ///     .arg(Arg::new("word")
    ///         .index(1))
    ///     .get_matches_from(vec![
    ///         "prog", "-F", "file1", "-F", "file2", "-F", "file3", "word"
    ///     ]);
    ///
    /// assert!(m.is_present("file"));
    /// let files: Vec<_> = m.values_of("file").unwrap().collect();
    /// assert_eq!(files, ["file1", "file2", "file3"]);
    /// assert!(m.is_present("word"));
    /// assert_eq!(m.value_of("word"), Some("word"));
    /// ```
    ///
    /// As a final example, let's fix the above error and get a pretty message to the user :)
    ///
    /// ```rust
    /// # use clap::{App, Arg, ErrorKind, ArgSettings};
    /// let res = App::new("prog")
    ///     .arg(Arg::new("file")
    ///         .setting(ArgSettings::MultipleOccurrences)
    ///         .setting(ArgSettings::TakesValue)
    ///         .short('F'))
    ///     .arg(Arg::new("word")
    ///         .index(1))
    ///     .try_get_matches_from(vec![
    ///         "prog", "-F", "file1", "file2", "file3", "word"
    ///     ]);
    ///
    /// assert!(res.is_err());
    /// assert_eq!(res.unwrap_err().kind, ErrorKind::UnknownArgument);
    /// ```
    ///
    /// [`subcommands`]: crate::App::subcommand()
    /// [`Arg::number_of_values(1)`]: Arg::number_of_values()
    /// [`MultipleOccurrences`]: ArgSettings::MultipleOccurrences
    /// [`MultipleValues`]: ArgSettings::MultipleValues
    /// [maximum number of values]: Arg::max_values()
    /// [specific number of values]: Arg::number_of_values()
    /// [maximum]: Arg::max_values()
    /// [specific]: Arg::number_of_values()
    #[inline]
    pub fn multiple_values(self, multi: bool) -> Self {
        if multi {
            self.setting(ArgSettings::MultipleValues)
        } else {
            self.unset_setting(ArgSettings::MultipleValues)
        }
    }

    /// Specifies that the argument may appear more than once. For flags, this results
    /// in the number of occurrences of the flag being recorded. For example `-ddd` or `-d -d -d`
    /// would count as three occurrences. For options or arguments that take a value, this
    /// *does not* affect how many values they can accept. (i.e. only one at a time is allowed)
    ///
    /// For example, `--opt val1 --opt val2` is allowed, but `--opt val1 val2` is not.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use clap::{App, Arg, ArgSettings};
    /// Arg::new("debug")
    ///     .short('d')
    ///     .setting(ArgSettings::MultipleOccurrences);
    /// ```
    ///
    /// An example with flags
    ///
    /// ```rust
    /// # use clap::{App, Arg, ArgSettings};
    /// let m = App::new("prog")
    ///     .arg(Arg::new("verbose")
    ///         .setting(ArgSettings::MultipleOccurrences)
    ///         .short('v'))
    ///     .get_matches_from(vec![
    ///         "prog", "-v", "-v", "-v"    // note, -vvv would have same result
    ///     ]);
    ///
    /// assert!(m.is_present("verbose"));
    /// assert_eq!(m.occurrences_of("verbose"), 3);
    /// ```
    ///
    /// An example with options
    ///
    /// ```rust
    /// # use clap::{App, Arg, ArgSettings};
    /// let m = App::new("prog")
    ///     .arg(Arg::new("file")
    ///         .setting(ArgSettings::MultipleOccurrences)
    ///         .setting(ArgSettings::TakesValue)
    ///         .short('F'))
    ///     .get_matches_from(vec![
    ///         "prog", "-F", "file1", "-F", "file2", "-F", "file3"
    ///     ]);
    ///
    /// assert!(m.is_present("file"));
    /// assert_eq!(m.occurrences_of("file"), 3);
    /// let files: Vec<_> = m.values_of("file").unwrap().collect();
    /// assert_eq!(files, ["file1", "file2", "file3"]);
    /// ```
    #[inline]
    pub fn multiple_occurrences(self, multi: bool) -> Self {
        if multi {
            self.setting(ArgSettings::MultipleOccurrences)
        } else {
            self.unset_setting(ArgSettings::MultipleOccurrences)
        }
    }

    /// Deprecated, see [`Arg::multiple_occurrences`] (most likely what you want) and
    /// [`Arg::multiple_values`]
    #[deprecated(
        since = "3.0.0",
        note = "Split into `Arg::multiple_occurrences` (most likely what you want)  and `Arg::multiple_values`"
    )]
    pub fn multiple(self, multi: bool) -> Self {
        self.multiple_occurrences(multi).multiple_values(multi)
    }

    /// Indicates that all parameters passed after this should not be parsed
    /// individually, but rather passed in their entirety. It is worth noting
    /// that setting this requires all values to come after a `--` to indicate they
    /// should all be captured. For example:
    ///
    /// ```text
    /// --foo something -- -v -v -v -b -b -b --baz -q -u -x
    /// ```
    ///
    /// Will result in everything after `--` to be considered one raw argument. This behavior
    /// may not be exactly what you are expecting and using [`crate::AppSettings::TrailingVarArg`]
    /// may be more appropriate.
    ///
    /// **NOTE:** Implicitly sets [`Arg::takes_value(true)`] [`Arg::multiple_values(true)`],
    /// [`Arg::allow_hyphen_values(true)`], and [`Arg::last(true)`] when set to `true`
    ///
    /// [`Arg::takes_value(true)`]: Arg::takes_value()
    /// [`Arg::multiple_values(true)`]: Arg::multiple_values()
    /// [`Arg::allow_hyphen_values(true)`]: Arg::allow_hyphen_values()
    /// [`Arg::last(true)`]: Arg::last()
    #[inline]
    pub fn raw(self, raw: bool) -> Self {
        self.takes_value(raw)
            .multiple_values(raw)
            .allow_hyphen_values(raw)
            .last(raw)
    }

    /// Hides an argument from short help message output.
    ///
    /// **NOTE:** This does **not** hide the argument from usage strings on error
    ///
    /// **NOTE:** Setting this option will cause next-line-help output style to be used
    /// when long help (`--help`) is called.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use clap::{App, Arg};
    /// Arg::new("debug")
    ///     .hidden_short_help(true);
    /// ```
    ///
    /// Setting `hidden_short_help(true)` will hide the argument when displaying short help text
    ///
    /// ```rust
    /// # use clap::{App, Arg};
    /// let m = App::new("prog")
    ///     .arg(Arg::new("cfg")
    ///         .long("config")
    ///         .hidden_short_help(true)
    ///         .about("Some help text describing the --config arg"))
    ///     .get_matches_from(vec![
    ///         "prog", "-h"
    ///     ]);
    /// ```
    ///
    /// The above example displays
    ///
    /// ```text
    /// helptest
    ///
    /// USAGE:
    ///    helptest [OPTIONS]
    ///
    /// OPTIONS:
    /// -h, --help       Print help information
    /// -V, --version    Print version information
    /// ```
    ///
    /// However, when --help is called
    ///
    /// ```rust
    /// # use clap::{App, Arg};
    /// let m = App::new("prog")
    ///     .arg(Arg::new("cfg")
    ///         .long("config")
    ///         .hidden_short_help(true)
    ///         .about("Some help text describing the --config arg"))
    ///     .get_matches_from(vec![
    ///         "prog", "--help"
    ///     ]);
    /// ```
    ///
    /// Then the following would be displayed
    ///
    /// ```text
    /// helptest
    ///
    /// USAGE:
    ///    helptest [OPTIONS]
    ///
    /// OPTIONS:
    ///     --config     Some help text describing the --config arg
    /// -h, --help       Print help information
    /// -V, --version    Print version information
    /// ```
    #[inline]
    pub fn hidden_short_help(self, hide: bool) -> Self {
        if hide {
            self.setting(ArgSettings::HiddenShortHelp)
        } else {
            self.unset_setting(ArgSettings::HiddenShortHelp)
        }
    }

    /// Hides an argument from long help message output.
    ///
    /// **NOTE:** This does **not** hide the argument from usage strings on error
    ///
    /// **NOTE:** Setting this option will cause next-line-help output style to be used
    /// when long help (`--help`) is called.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use clap::{App, Arg};
    /// Arg::new("debug")
    ///     .hidden_long_help(true)
    /// # ;
    /// ```
    /// Setting `hidden_long_help(true)` will hide the argument when displaying long help text
    ///
    /// ```rust
    /// # use clap::{App, Arg};
    /// let m = App::new("prog")
    ///     .arg(Arg::new("cfg")
    ///         .long("config")
    ///         .hidden_long_help(true)
    ///         .about("Some help text describing the --config arg"))
    ///     .get_matches_from(vec![
    ///         "prog", "--help"
    ///     ]);
    /// ```
    ///
    /// The above example displays
    ///
    /// ```text
    /// helptest
    ///
    /// USAGE:
    ///    helptest [OPTIONS]
    ///
    /// OPTIONS:
    /// -h, --help       Print help information
    /// -V, --version    Print version information
    /// ```
    ///
    /// However, when -h is called
    ///
    /// ```rust
    /// # use clap::{App, Arg};
    /// let m = App::new("prog")
    ///     .arg(Arg::new("cfg")
    ///         .long("config")
    ///         .hidden_long_help(true)
    ///         .about("Some help text describing the --config arg"))
    ///     .get_matches_from(vec![
    ///         "prog", "-h"
    ///     ]);
    /// ```
    ///
    /// Then the following would be displayed
    ///
    /// ```text
    /// helptest
    ///
    /// USAGE:
    ///    helptest [OPTIONS]
    ///
    /// OPTIONS:
    ///     --config     Some help text describing the --config arg
    /// -h, --help       Print help information
    /// -V, --version    Print version information
    /// ```
    #[inline]
    pub fn hidden_long_help(self, hide: bool) -> Self {
        if hide {
            self.setting(ArgSettings::HiddenLongHelp)
        } else {
            self.unset_setting(ArgSettings::HiddenLongHelp)
        }
    }

    // @TODO @docs @v3-beta: write better docs as ArgSettings is now critical
    /// Checks if one of the [`ArgSettings`] is set for the argument
    ///
    #[inline]
    pub fn is_set(&self, s: ArgSettings) -> bool {
        self.settings.is_set(s)
    }

    /// Enables a single setting for the current (this `Arg` instance) argument.
    ///
    /// See [`ArgSettings`] for a full list of possibilities and examples.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use clap::{Arg, ArgSettings};
    /// Arg::new("config")
    ///     .setting(ArgSettings::Required)
    ///     .setting(ArgSettings::TakesValue)
    /// # ;
    /// ```
    ///
    /// ```no_run
    /// # use clap::{Arg, ArgSettings};
    /// Arg::new("config")
    ///     .setting(ArgSettings::Required | ArgSettings::TakesValue)
    /// # ;
    /// ```
    #[inline]
    pub fn setting<F>(mut self, setting: F) -> Self
    where
        F: Into<ArgFlags>,
    {
        self.settings.insert(setting.into());
        self
    }

    /// Deprecated, see [`Arg::setting`]
    #[deprecated(since = "3.0.0", note = "Replaced with `Arg::setting`")]
    pub fn set(self, s: ArgSettings) -> Self {
        self.setting(s)
    }

    /// Disables a single setting for the current (this `Arg` instance) argument.
    ///
    /// See [`ArgSettings`] for a full list of possibilities and examples.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use clap::{Arg, ArgSettings};
    /// Arg::new("config")
    ///     .unset_setting(ArgSettings::Required)
    ///     .unset_setting(ArgSettings::TakesValue)
    /// # ;
    /// ```
    ///
    /// ```no_run
    /// # use clap::{Arg, ArgSettings};
    /// Arg::new("config")
    ///     .unset_setting(ArgSettings::Required | ArgSettings::TakesValue)
    /// # ;
    /// ```
    #[inline]
    pub fn unset_setting<F>(mut self, setting: F) -> Self
    where
        F: Into<ArgFlags>,
    {
        self.settings.remove(setting.into());
        self
    }

    /// Deprecated, see [`Arg::unset_setting`]
    #[deprecated(since = "3.0.0", note = "Replaced with `Arg::unset_setting`")]
    pub fn unset(self, s: ArgSettings) -> Self {
        self.unset_setting(s)
    }

    /// Set a custom heading for this arg to be printed under
    #[inline]
    pub fn help_heading<O>(mut self, heading: O) -> Self
    where
        O: Into<Option<&'help str>>,
    {
        self.help_heading = Some(heading.into());
        self
    }

    /// Sets a hint about the type of the value for shell completions
    ///
    /// Currently this is only supported by the zsh completions generator.
    ///
    /// **NOTE:** implicitly sets [`Arg::takes_value(true)`].
    ///
    /// For example, to take a username as argument:
    ///
    /// ```
    /// # use clap::{Arg, ValueHint};
    /// Arg::new("user")
    ///     .short('u')
    ///     .long("user")
    ///     .value_hint(ValueHint::Username);
    /// ```
    ///
    /// To take a full command line and its arguments (for example, when writing a command wrapper):
    ///
    /// ```
    /// # use clap::{App, AppSettings, Arg, ValueHint};
    /// App::new("prog")
    ///     .setting(AppSettings::TrailingVarArg)
    ///     .arg(
    ///         Arg::new("command")
    ///             .takes_value(true)
    ///             .multiple_values(true)
    ///             .value_hint(ValueHint::CommandWithArguments)
    ///     );
    /// ```
    pub fn value_hint(mut self, value_hint: ValueHint) -> Self {
        self.value_hint = value_hint;
        self.takes_value(true)
    }

    pub(crate) fn _build(&mut self) {
        if self.is_positional() {
            self.settings.set(ArgSettings::TakesValue);
        }

        if (self.is_set(ArgSettings::UseValueDelimiter)
            || self.is_set(ArgSettings::RequireDelimiter))
            && self.val_delim.is_none()
        {
            self.val_delim = Some(',');
        }

        let val_names_len = self.val_names.len();

        if val_names_len > 1 {
            self.settings.set(ArgSettings::MultipleValues);

            if self.num_vals.is_none() {
                self.num_vals = Some(val_names_len);
            }
        }
    }

    pub(crate) fn longest_filter(&self) -> bool {
        self.is_set(ArgSettings::TakesValue) || self.long.is_some() || self.short.is_none()
    }

    // Used for positionals when printing
    pub(crate) fn multiple_str(&self) -> &str {
        let mult_vals = self.val_names.len() > 1;
        if (self.is_set(ArgSettings::MultipleValues)
            || self.is_set(ArgSettings::MultipleOccurrences))
            && !mult_vals
        {
            "..."
        } else {
            ""
        }
    }

    // Used for positionals when printing
    pub(crate) fn name_no_brackets(&self) -> Cow<str> {
        debug!("Arg::name_no_brackets:{}", self.name);
        let mut delim = String::new();
        delim.push(if self.is_set(ArgSettings::RequireDelimiter) {
            self.val_delim.expect(INTERNAL_ERROR_MSG)
        } else {
            ' '
        });
        if !self.val_names.is_empty() {
            debug!("Arg::name_no_brackets: val_names={:#?}", self.val_names);

            if self.val_names.len() > 1 {
                Cow::Owned(
                    self.val_names
                        .iter()
                        .map(|n| format!("<{}>", n))
                        .collect::<Vec<_>>()
                        .join(&*delim),
                )
            } else {
                Cow::Borrowed(self.val_names.get(0).expect(INTERNAL_ERROR_MSG))
            }
        } else {
            debug!("Arg::name_no_brackets: just name");
            Cow::Borrowed(self.name)
        }
    }
}

#[cfg(feature = "yaml")]
impl<'help> From<&'help Yaml> for Arg<'help> {
    /// Creates a new instance of [`Arg`] from a .yaml (YAML) file.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use clap::{Arg, load_yaml};
    /// let yaml = load_yaml!("arg.yaml");
    /// let arg = Arg::from(yaml);
    /// ```
    #[allow(clippy::cognitive_complexity)]
    fn from(y: &'help Yaml) -> Self {
        let yaml_file_hash = y.as_hash().expect("YAML file must be a hash");
        // We WANT this to panic on error...so expect() is good.
        let (name_yaml, yaml) = yaml_file_hash
            .iter()
            .next()
            .expect("There must be one arg in the YAML file");
        let name_str = name_yaml.as_str().expect("Arg name must be a string");
        let mut a = Arg::new(name_str);

        let mut has_metadata = false;

        for (k, v) in yaml.as_hash().expect("Arg must be a hash") {
            a = match k.as_str().expect("Arg fields must be strings") {
                "_has_metadata" => {
                    has_metadata = true;
                    a
                }
                "short" => yaml_to_char!(a, v, short),
                "long" => yaml_to_str!(a, v, long),
                "alias" => yaml_to_str!(a, v, alias),
                "aliases" => yaml_vec_or_str!(a, v, alias),
                "short_alias" => yaml_to_str!(a, v, alias),
                "short_aliases" => yaml_to_chars!(a, v, short_aliases),
                "about" => yaml_to_str!(a, v, about),
                "long_about" => yaml_to_str!(a, v, long_about),
                "required" => yaml_to_bool!(a, v, required),
                "required_if_eq" => yaml_tuple2!(a, v, required_if_eq),
                "required_if_eq_any" => yaml_array_tuple2!(a, v, required_if_eq_any),
                "required_if_eq_all" => yaml_array_tuple2!(a, v, required_if_eq_all),
                "takes_value" => yaml_to_bool!(a, v, takes_value),
                "index" => yaml_to_usize!(a, v, index),
                "global" => yaml_to_bool!(a, v, global),
                "multiple_occurrences" => yaml_to_bool!(a, v, multiple_occurrences),
                "multiple_values" => yaml_to_bool!(a, v, multiple_values),
                "hidden" => yaml_to_bool!(a, v, hidden),
                "hidden_long_help" => yaml_to_bool!(a, v, hidden_long_help),
                "hidden_short_help" => yaml_to_bool!(a, v, hidden_short_help),
                "next_line_help" => yaml_to_bool!(a, v, next_line_help),
                "group" => yaml_to_str!(a, v, group),
                "number_of_values" => yaml_to_usize!(a, v, number_of_values),
                "max_values" => yaml_to_usize!(a, v, max_values),
                "min_values" => yaml_to_usize!(a, v, min_values),
                "value_name" => yaml_to_str!(a, v, value_name),
                "use_delimiter" => yaml_to_bool!(a, v, use_delimiter),
                "allow_hyphen_values" => yaml_to_bool!(a, v, allow_hyphen_values),
                "raw" => yaml_to_bool!(a, v, raw),
                "require_equals" => yaml_to_bool!(a, v, require_equals),
                "require_delimiter" => yaml_to_bool!(a, v, require_delimiter),
                "value_terminator" => yaml_to_str!(a, v, value_terminator),
                "value_delimiter" => yaml_to_char!(a, v, value_delimiter),
                "required_unless_present" => yaml_to_str!(a, v, required_unless_present),
                "display_order" => yaml_to_usize!(a, v, display_order),
                "default_value" => yaml_to_str!(a, v, default_value),
                "default_value_if" => yaml_tuple3!(a, v, default_value_if),
                "default_value_ifs" => yaml_tuple3!(a, v, default_value_if),
                "default_missing_value" => yaml_to_str!(a, v, default_missing_value),
                #[cfg(feature = "env")]
                "env" => yaml_to_str!(a, v, env),
                "value_names" => yaml_vec_or_str!(a, v, value_name),
                "groups" => yaml_vec_or_str!(a, v, group),
                "requires" => yaml_vec_or_str!(a, v, requires),
                "requires_if" => yaml_tuple2!(a, v, requires_if),
                "requires_ifs" => yaml_tuple2!(a, v, requires_if),
                "conflicts_with" => yaml_vec_or_str!(a, v, conflicts_with),
                "exclusive" => yaml_to_bool!(a, v, exclusive),
                "last" => yaml_to_bool!(a, v, last),
                "help_heading" => yaml_to_str!(a, v, help_heading),
                "value_hint" => yaml_str_parse!(a, v, value_hint),
                "hide_default_value" => yaml_to_bool!(a, v, hide_default_value),
                #[cfg(feature = "env")]
                "hide_env" => yaml_to_bool!(a, v, hide_env),
                #[cfg(feature = "env")]
                "hide_env_values" => yaml_to_bool!(a, v, hide_env_values),
                "hide_possible_values" => yaml_to_bool!(a, v, hide_possible_values),
                "overrides_with" => yaml_to_str!(a, v, overrides_with),
                "overrides_with_all" => yaml_vec_or_str!(a, v, overrides_with),
                "possible_value" => yaml_to_str!(a, v, possible_value),
                "possible_values" => yaml_vec_or_str!(a, v, possible_value),
                "case_insensitive" => yaml_to_bool!(a, v, case_insensitive),
                "required_unless_present_any" => yaml_vec!(a, v, required_unless_present_any),
                "required_unless_present_all" => yaml_vec!(a, v, required_unless_present_all),
                "visible_alias" => yaml_to_str!(a, v, visible_alias),
                "visible_aliases" => yaml_vec_or_str!(a, v, visible_alias),
                "visible_short_alias" => yaml_to_char!(a, v, visible_short_alias),
                "visible_short_aliases" => yaml_to_chars!(a, v, visible_short_aliases),
                #[cfg(feature = "regex")]
                "validator_regex" => {
                    if let Some(vec) = v.as_vec() {
                        debug_assert_eq!(2, vec.len());
                        let regex = yaml_str!(vec[0]);

                        match Regex::new(regex) {
                            Err(e) => panic!(
                                "Failed to convert \"{}\" into regular expression: {}",
                                regex, e
                            ),
                            Ok(regex) => a.validator_regex(regex, yaml_str!(vec[1])),
                        }
                    } else {
                        panic!("Failed to convert YAML value to vector")
                    }
                }
                "setting" | "settings" => {
                    yaml_to_setting!(
                        a,
                        v,
                        setting,
                        ArgSettings,
                        "ArgSetting",
                        format!("arg '{}'", name_str)
                    )
                }
                s => {
                    if !has_metadata {
                        panic!(
                            "Unknown setting '{}' in YAML file for arg '{}'",
                            s, name_str
                        )
                    }
                    continue;
                }
            }
        }

        a
    }
}

impl<'help> From<&'_ Arg<'help>> for Arg<'help> {
    fn from(a: &Arg<'help>) -> Self {
        a.clone()
    }
}

impl<'help> From<&'help str> for Arg<'help> {
    fn from(s: &'help str) -> Self {
        UsageParser::from_usage(s).parse()
    }
}

impl<'help> PartialEq for Arg<'help> {
    fn eq(&self, other: &Arg<'help>) -> bool {
        self.name == other.name
    }
}

/// Write the values such as <name1> <name2>
pub(crate) fn display_arg_val<F, T, E>(arg: &Arg, mut write: F) -> Result<(), E>
where
    F: FnMut(&str, bool) -> Result<T, E>,
{
    let mult_val = arg.is_set(ArgSettings::MultipleValues);
    let mult_occ = arg.is_set(ArgSettings::MultipleOccurrences);
    let delim = if arg.is_set(ArgSettings::RequireDelimiter) {
        arg.val_delim.expect(INTERNAL_ERROR_MSG)
    } else {
        ' '
    };
    if !arg.val_names.is_empty() {
        // If have val_name.
        match (arg.val_names.len(), arg.num_vals) {
            (1, Some(num_vals)) => {
                // If single value name with multiple num_of_vals, display all
                // the values with the single value name.
                let arg_name = format!("<{}>", arg.val_names.get(0).unwrap());
                let mut it = iter::repeat(arg_name).take(num_vals).peekable();
                while let Some(arg_name) = it.next() {
                    write(&arg_name, true)?;
                    if it.peek().is_some() {
                        write(&delim.to_string(), false)?;
                    }
                }
            }
            (num_val_names, _) => {
                // If multiple value names, display them sequentially(ignore num of vals).
                let mut it = arg.val_names.iter().peekable();
                while let Some(val) = it.next() {
                    write(&format!("<{}>", val), true)?;
                    if it.peek().is_some() {
                        write(&delim.to_string(), false)?;
                    }
                }
                if num_val_names == 1 && mult_val {
                    write("...", true)?;
                }
            }
        }
    } else if let Some(num_vals) = arg.num_vals {
        // If number_of_values is sepcified, display the value multiple times.
        let arg_name = format!("<{}>", arg.name);
        let mut it = iter::repeat(&arg_name).take(num_vals).peekable();
        while let Some(arg_name) = it.next() {
            write(arg_name, true)?;
            if it.peek().is_some() {
                write(&delim.to_string(), false)?;
            }
        }
    } else if arg.is_positional() {
        // Value of positional argument with no num_vals and val_names.
        write(&format!("<{}>", arg.name), true)?;

        if mult_val || mult_occ {
            write("...", true)?;
        }
    } else {
        // value of flag argument with no num_vals and val_names.
        write(&format!("<{}>", arg.name), true)?;
        if mult_val {
            write("...", true)?;
        }
    }
    Ok(())
}

impl<'help> Display for Arg<'help> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        // Write the name such --long or -l
        if let Some(l) = self.long {
            write!(f, "--{}", l)?;
        } else if let Some(s) = self.short {
            write!(f, "-{}", s)?;
        }
        if !self.is_positional() && self.is_set(ArgSettings::TakesValue) {
            let sep = if self.is_set(ArgSettings::RequireEquals) {
                "="
            } else {
                " "
            };
            write!(f, "{}", sep)?;
        }
        if self.is_set(ArgSettings::TakesValue) || self.is_positional() {
            display_arg_val(self, |s, _| write!(f, "{}", s))?;
        }

        Ok(())
    }
}

impl<'help> PartialOrd for Arg<'help> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<'help> Ord for Arg<'help> {
    fn cmp(&self, other: &Arg) -> Ordering {
        self.name.cmp(other.name)
    }
}

impl<'help> Eq for Arg<'help> {}

impl<'help> fmt::Debug for Arg<'help> {
    fn fmt(&self, f: &mut Formatter) -> Result<(), fmt::Error> {
        let mut ds = f.debug_struct("Arg");

        #[allow(unused_mut)]
        let mut ds = ds
            .field("id", &self.id)
            .field("provider", &self.provider)
            .field("name", &self.name)
            .field("about", &self.about)
            .field("long_about", &self.long_about)
            .field("blacklist", &self.blacklist)
            .field("settings", &self.settings)
            .field("overrides", &self.overrides)
            .field("groups", &self.groups)
            .field("requires", &self.requires)
            .field("r_ifs", &self.r_ifs)
            .field("r_unless", &self.r_unless)
            .field("short", &self.short)
            .field("long", &self.long)
            .field("aliases", &self.aliases)
            .field("short_aliases", &self.short_aliases)
            .field("disp_ord", &self.disp_ord)
            .field("unified_ord", &self.unified_ord)
            .field("possible_vals", &self.possible_vals)
            .field("val_names", &self.val_names)
            .field("num_vals", &self.num_vals)
            .field("max_vals", &self.max_vals)
            .field("min_vals", &self.min_vals)
            .field(
                "validator",
                &self.validator.as_ref().map_or("None", |_| "Some(FnMut)"),
            )
            .field(
                "validator_os",
                &self.validator_os.as_ref().map_or("None", |_| "Some(FnMut)"),
            )
            .field("val_delim", &self.val_delim)
            .field("default_vals", &self.default_vals)
            .field("default_vals_ifs", &self.default_vals_ifs)
            .field("terminator", &self.terminator)
            .field("index", &self.index)
            .field("help_heading", &self.help_heading)
            .field("global", &self.global)
            .field("exclusive", &self.exclusive)
            .field("value_hint", &self.value_hint)
            .field("default_missing_vals", &self.default_missing_vals);

        #[cfg(feature = "env")]
        {
            ds = ds.field("env", &self.env);
        }

        ds.finish()
    }
}

// Flags
#[cfg(test)]
mod test {
    use super::Arg;
    use crate::build::ArgSettings;

    #[test]
    fn flag_display() {
        let mut f = Arg::new("flg").setting(ArgSettings::MultipleOccurrences);
        f.long = Some("flag");

        assert_eq!(&*format!("{}", f), "--flag");

        let mut f2 = Arg::new("flg");
        f2.short = Some('f');

        assert_eq!(&*format!("{}", f2), "-f");
    }

    #[test]
    fn flag_display_single_alias() {
        let mut f = Arg::new("flg");
        f.long = Some("flag");
        f.aliases = vec![("als", true)];

        assert_eq!(&*format!("{}", f), "--flag")
    }

    #[test]
    fn flag_display_multiple_aliases() {
        let mut f = Arg::new("flg");
        f.short = Some('f');
        f.aliases = vec![
            ("alias_not_visible", false),
            ("f2", true),
            ("f3", true),
            ("f4", true),
        ];
        assert_eq!(&*format!("{}", f), "-f");
    }

    #[test]
    fn flag_display_single_short_alias() {
        let mut f = Arg::new("flg");
        f.short = Some('a');
        f.short_aliases = vec![('b', true)];

        assert_eq!(&*format!("{}", f), "-a")
    }

    #[test]
    fn flag_display_multiple_short_aliases() {
        let mut f = Arg::new("flg");
        f.short = Some('a');
        f.short_aliases = vec![('b', false), ('c', true), ('d', true), ('e', true)];
        assert_eq!(&*format!("{}", f), "-a");
    }

    // Options

    #[test]
    fn option_display_multiple_occurrences() {
        let o = Arg::new("opt")
            .long("option")
            .takes_value(true)
            .multiple_occurrences(true);

        assert_eq!(&*format!("{}", o), "--option <opt>");
    }

    #[test]
    fn option_display_multiple_values() {
        let o = Arg::new("opt")
            .long("option")
            .takes_value(true)
            .multiple_values(true);

        assert_eq!(&*format!("{}", o), "--option <opt>...");
    }

    #[test]
    fn option_display2() {
        let o2 = Arg::new("opt").short('o').value_names(&["file", "name"]);

        assert_eq!(&*format!("{}", o2), "-o <file> <name>");
    }

    #[test]
    fn option_display3() {
        let o2 = Arg::new("opt")
            .short('o')
            .takes_value(true)
            .multiple_values(true)
            .value_names(&["file", "name"]);

        assert_eq!(&*format!("{}", o2), "-o <file> <name>");
    }

    #[test]
    fn option_display_single_alias() {
        let o = Arg::new("opt")
            .takes_value(true)
            .long("option")
            .visible_alias("als");

        assert_eq!(&*format!("{}", o), "--option <opt>");
    }

    #[test]
    fn option_display_multiple_aliases() {
        let o = Arg::new("opt")
            .long("option")
            .takes_value(true)
            .visible_aliases(&["als2", "als3", "als4"])
            .alias("als_not_visible");

        assert_eq!(&*format!("{}", o), "--option <opt>");
    }

    #[test]
    fn option_display_single_short_alias() {
        let o = Arg::new("opt")
            .takes_value(true)
            .short('a')
            .visible_short_alias('b');

        assert_eq!(&*format!("{}", o), "-a <opt>");
    }

    #[test]
    fn option_display_multiple_short_aliases() {
        let o = Arg::new("opt")
            .short('a')
            .takes_value(true)
            .visible_short_aliases(&['b', 'c', 'd'])
            .short_alias('e');

        assert_eq!(&*format!("{}", o), "-a <opt>");
    }

    // Positionals

    #[test]
    fn positional_display_multiple_values() {
        let p = Arg::new("pos")
            .index(1)
            .setting(ArgSettings::TakesValue)
            .setting(ArgSettings::MultipleValues);

        assert_eq!(&*format!("{}", p), "<pos>...");
    }

    #[test]
    fn positional_display_multiple_occurrences() {
        let p = Arg::new("pos")
            .index(1)
            .setting(ArgSettings::TakesValue)
            .setting(ArgSettings::MultipleOccurrences);

        assert_eq!(&*format!("{}", p), "<pos>...");
    }

    #[test]
    fn positional_display_required() {
        let p2 = Arg::new("pos").index(1).setting(ArgSettings::Required);

        assert_eq!(&*format!("{}", p2), "<pos>");
    }

    #[test]
    fn positional_display_val_names() {
        let p2 = Arg::new("pos").index(1).value_names(&["file1", "file2"]);

        assert_eq!(&*format!("{}", p2), "<file1> <file2>");
    }

    #[test]
    fn positional_display_val_names_req() {
        let p2 = Arg::new("pos")
            .index(1)
            .setting(ArgSettings::Required)
            .value_names(&["file1", "file2"]);

        assert_eq!(&*format!("{}", p2), "<file1> <file2>");
    }
}
