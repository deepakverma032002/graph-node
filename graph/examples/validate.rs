/// Validate subgraph schemas by parsing them into `InputSchema` and making
/// sure that they are valid
///
/// The input files must be in a particular format; that can be generated by
/// running this script against graph-node shard(s). Before running it,
/// change the `dbs` variable to list all databases against which it should
/// run.
///
/// ```
/// #! /bin/bash
///
/// read -r -d '' query <<EOF
/// \copy (select to_jsonb(a.*) from (select id, schema from subgraphs.subgraph_manifest) a) to '%s'
/// EOF
///
/// dbs="shard1 shard2 .."
///
/// dir=/var/tmp/schemas
/// mkdir -p $dir
///
/// for db in $dbs
/// do
///     echo "Dump $db"
///     q=$(printf "$query" "$dir/$db.json")
///     psql -qXt service=$db -c "$q"
/// done
///
/// ```
use graph::data::graphql::ext::DirectiveFinder;
use graph::data::graphql::DirectiveExt;
use graph::data::graphql::DocumentExt;
use graph::prelude::s;
use graph::prelude::DeploymentHash;
use graph::schema::InputSchema;
use graphql_parser::parse_schema;
use serde::Deserialize;
use std::env;
use std::fs::File;
use std::io::BufRead;
use std::io::BufReader;
use std::process::exit;

pub fn usage(msg: &str) -> ! {
    println!("{}", msg);
    println!("usage: validate schema.graphql ...");
    println!("\nValidate subgraph schemas");
    std::process::exit(1);
}

pub fn ensure<T, E: std::fmt::Display>(res: Result<T, E>, msg: &str) -> T {
    match res {
        Ok(ok) => ok,
        Err(err) => {
            eprintln!("{}:\n    {}", msg, err);
            exit(1)
        }
    }
}

fn subgraph_id(schema: &s::Document) -> DeploymentHash {
    let id = schema
        .get_object_type_definitions()
        .first()
        .and_then(|obj_type| obj_type.find_directive("subgraphId"))
        .and_then(|dir| dir.argument("id"))
        .and_then(|arg| match arg {
            s::Value::String(s) => Some(s.to_owned()),
            _ => None,
        })
        .unwrap_or("unknown".to_string());
    DeploymentHash::new(id).expect("subgraph id is not a valid deployment hash")
}

#[derive(Deserialize)]
struct Entry {
    id: i32,
    schema: String,
}

pub fn main() {
    // Allow fulltext search in schemas
    std::env::set_var("GRAPH_ALLOW_NON_DETERMINISTIC_FULLTEXT_SEARCH", "true");

    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        usage("please provide a subgraph schema");
    }
    for arg in &args[1..] {
        println!("Validating schemas from {arg}");
        let file = File::open(arg).expect("file exists");
        let rdr = BufReader::new(file);
        for line in rdr.lines() {
            let line = line.expect("invalid line").replace("\\\\", "\\");
            let entry = serde_json::from_str::<Entry>(&line).expect("line is valid json");

            let raw = &entry.schema;
            let schema = ensure(
                parse_schema(raw).map(|v| v.into_static()),
                &format!("Failed to parse schema sgd{}", entry.id),
            );
            let id = subgraph_id(&schema);
            match InputSchema::parse(raw, id.clone()) {
                Ok(_) => println!("sgd{}[{}]: OK", entry.id, id),
                Err(e) => println!("sgd{}[{}]: {}", entry.id, id, e),
            }
        }
    }
}