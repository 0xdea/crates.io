use crate::models::{Crate, CrateVersions, Dependency, Version};
use crate::schema::{crates, dependencies};
use crate::util::diesel::Conn;
use anyhow::Context;
use crates_io_index::features::split_features;
use diesel::prelude::*;
use sentry::Level;

#[instrument(skip_all, fields(krate.name = ?name))]
pub fn get_index_data(name: &str, conn: &mut impl Conn) -> anyhow::Result<Option<String>> {
    debug!("Looking up crate by name");
    let Some(krate): Option<Crate> = Crate::by_exact_name(name).first(conn).optional()? else {
        return Ok(None);
    };

    debug!("Gathering remaining index data");
    let crates = index_metadata(&krate, conn).context("Failed to gather index metadata")?;

    // This can sometimes happen when we delete versions upon owner request
    // but don't realize that the crate is now left with no versions at all.
    //
    // In this case we will delete the crate from the index and log a warning to
    // Sentry to clean this up in the database.
    if crates.is_empty() {
        let message = format!("Crate `{name}` has no versions left");
        sentry::capture_message(&message, Level::Warning);

        return Ok(None);
    }

    debug!("Serializing index data");
    let mut bytes = Vec::new();
    crates_io_index::write_crates(&crates, &mut bytes)
        .context("Failed to serialize index metadata")?;

    let str = String::from_utf8(bytes).context("Failed to decode index metadata as utf8")?;

    Ok(Some(str))
}

/// Gather all the necessary data to write an index metadata file
pub fn index_metadata(
    krate: &Crate,
    conn: &mut impl Conn,
) -> QueryResult<Vec<crates_io_index::Crate>> {
    let mut versions: Vec<Version> = krate.all_versions().load(conn)?;

    // We sort by `created_at` by default, but since tests run within a
    // single database transaction the versions will all have the same
    // `created_at` timestamp, so we sort by semver as a secondary key.
    versions.sort_by_cached_key(|k| (k.created_at, semver::Version::parse(&k.num).ok()));

    let deps: Vec<(Dependency, String)> = Dependency::belonging_to(&versions)
        .inner_join(crates::table)
        .select((dependencies::all_columns, crates::name))
        .load(conn)?;

    let deps = deps.grouped_by(&versions);

    versions
        .into_iter()
        .zip(deps)
        .map(|(version, deps)| {
            let mut deps = deps
                .into_iter()
                .map(|(dep, name)| {
                    // If this dependency has an explicit name in `Cargo.toml` that
                    // means that the `name` we have listed is actually the package name
                    // that we're depending on. The `name` listed in the index is the
                    // Cargo.toml-written-name which is what cargo uses for
                    // `--extern foo=...`
                    let (name, package) = match dep.explicit_name {
                        Some(explicit_name) => (explicit_name, Some(name)),
                        None => (name, None),
                    };

                    crates_io_index::Dependency {
                        name,
                        req: dep.req,
                        features: dep.features,
                        optional: dep.optional,
                        default_features: dep.default_features,
                        kind: Some(dep.kind.into()),
                        package,
                        target: dep.target,
                    }
                })
                .collect::<Vec<_>>();

            deps.sort();

            let features = version.features().unwrap_or_default();
            let (features, features2) = split_features(features);

            let (features2, v) = if features2.is_empty() {
                (None, None)
            } else {
                (Some(features2), Some(2))
            };

            let krate = crates_io_index::Crate {
                name: krate.name.clone(),
                vers: version.num.to_string(),
                cksum: version.checksum,
                yanked: Some(version.yanked),
                deps,
                features,
                links: version.links,
                rust_version: version.rust_version,
                features2,
                v,
            };

            Ok(krate)
        })
        .collect()
}
