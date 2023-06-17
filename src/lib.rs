use toml_edit::Document;
use worker::{console_log, FormEntry, Response, Result};

struct Entry {
    key: String,
    storage: worker::Bucket,
}

#[worker::event(fetch)]
pub async fn main(
    mut req: worker::Request,
    env: worker::Env,
    _ctx: worker::Context,
) -> Result<Response> {
    console_log!("Hello ?");
    if req.method() != worker::Method::Post {
        return Response::ok("Hello, \n\nglaucus user, the syntax is:\n
index:
    wget -qO- --post-data 'action=get&password=glaucus'  https://glaucus.navediew.uk/index
or  curl -F action=get -F password=glaucus https://glaucus.navediew.uk/index\n
download:
    wget2 --post-data='action=get&password=glaucus' https://glaucus.navediew.uk/PACKAGENAME
or  curl -O -F action=get -F password=glaucus https://glaucus.navediew.uk/PACKAGENAME \n

add or update: curl -F action=set -F file=@PACKAGE_TO_UPLOAD  -F b3sum=BLAKE3_HASH -F version=PACKAGE_VERSIOM -F password=YOUR_ADMIN_PASSWORD https://glaucus.navediew.uk/PACKAGENAME\n
delete: curl -F action=del -F password=YOUR_ADMIN_PASSWORD https://glaucus.navediew.uk/PACKAGENAME");
    }
    let entry = Entry {
        key: req.path()[1..].to_string(),
        storage: env.bucket("glaucus")?,
    };
    let form = req.form_data().await?;
    let Some(FormEntry::Field(action)) = form.get("action") else{return Response::error("No action field", 400)};
    let Some(FormEntry::Field(password)) = form.get("password") else{return Response::error("No password field", 400)};
    return match action.as_str() {
        "get" => {
            if password != "glaucus" {
                return Response::error("Bad password, use 'glaucus'", 401);
            }
            entry.get().await
        }
        "del" => {
            if password != env.secret("adminpass")?.to_string() {
                return Response::error("Bad ADMIN password", 401);
            }
            entry.del().await
        }
        "set" => {
            if password != env.secret("adminpass")?.to_string() {
                return Response::error("Bad ADMIN password", 401);
            }
            let Some(FormEntry::File(file)) = form.get("file") else {return Response::error("no 'file' field", 400)};
            let Some(FormEntry::Field(cs)) = form.get("b3sum") else {return Response::error("no 'b3sum' field", 400)};
            let Some(FormEntry::Field(version)) = form.get("version") else {return Response::error("no 'version' field", 400)};
            entry.set(file, version, cs).await
        }
        _ => Response::error("Bad action field", 400),
    };
}

fn aaa(version: &str, b3: &str) -> toml_edit::Item {
    let mut entry = toml_edit::InlineTable::new();
    entry.insert("version", version.into());
    entry.insert("b3sum", b3.into());
    toml_edit::value(toml_edit::Value::InlineTable(entry))
}

impl Entry {
    async fn getindex(&self) -> Result<Document> {
        let answer = self.storage.get("index").execute().await?.unwrap();
        let file = answer.body().unwrap().text().await?;
        Ok(file.parse().expect("invalid toml format"))
    }
    async fn set(&self, retfile: worker::File, version: String, cs: String) -> Result<Response> {
        if self.key == "index" {
            return Response::error("The index cannot be modified", 401);
        }
        let file = retfile.bytes().await?;
        let b3 = blake3::hash(&file).to_string();
        if cs != b3 {
            return Response::error(format!("blake3: given {cs}, computed {b3}\n"), 401);
        }

        self.storage.put(&self.key, file).execute().await?;

        let mut doc = self.getindex().await?;
        doc[&self.key] = aaa(&version, &cs);
        self.storage.put("index", doc.to_string()).execute().await?;

        console_log!("set {}", self.key);
        Response::ok(format!("{} set successfully\n", self.key))
    }
    async fn del(&self) -> Result<Response> {
        if self.key == "index" {
            return Response::error("The index cannot be modified", 401);
        }

        self.storage.delete(&self.key).await?;

        let mut doc = self.getindex().await?;
        doc.remove_entry(&self.key);
        self.storage.put("index", doc.to_string()).execute().await?;

        console_log!("deleted {}", self.key);
        Response::ok(format!("{} deleted successfully\n", self.key))
    }
    async fn get(&self) -> Result<Response> {
        console_log!("fetching {}", self.key);
        if let Some(obj) = self.storage.get(&self.key).execute().await? {
            if let Some(objbody) = obj.body() {
                return if self.key == "index" {
                    Response::ok(objbody.text().await?)
                } else {
                    Response::from_bytes(objbody.bytes().await?)
                };
            }
        }
        Response::error("No file here", 404)
    }
}
