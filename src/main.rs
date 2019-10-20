use failure::format_err;
use reqwest::{
    header::{HeaderMap, HeaderValue},
    Client,
};
use scraper::{html::Html, Selector};

mod anime;
use anime::{Anime, Episodio, Servidor};

fn main() {
    fern::Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!(
                "[{}][{}] {}",
                record.target(),
                record.level(),
                message,
            ))
        })
        .level(log::LevelFilter::Info)
        .chain(std::io::stdout())
        .chain(fern::log_file("log.txt").expect("could not create log file"))
        .apply()
        .expect("could not apply log settings");

    let rt = tokio::runtime::Runtime::new().expect("could not create tokio runtime");

    rt.block_on(main_async()).expect("panicked on block_on");
    rt.shutdown_on_idle();
}

async fn main_async() -> Result<(), failure::Error> {
    let builder = reqwest::Client::builder();

    let mut headers = HeaderMap::new();
    headers.insert(
        "User-Agent",
        HeaderValue::from_str("Mozilla/5.0 (Macintosh; Intel Mac OS X 10.14; rv:69.0) Gecko/20100101 Firefox/69.0").expect("could not create header"),
    );
    headers.insert(
        "Cookie",
        HeaderValue::from_str(
            "__cfduid=d44a5b4504cc07b06a449170a71c2ba001571386115; _gat_gtag_UA_93274214_5=1;_ga=GA1.2.2073919087.1571467207; _gid=GA1.2.368526109.1571467207; AdskeeperStorage=%7B%220%22%3A%7B%22svspr%22%3A%22https%3A%2F%2Fmonoschinos.com%2F%22%2C%22svsds%22%3A1%2C%22TejndEEDj%22%3A%22-oHRSh5P*%22%7D%2C%22C375216%22%3A%7B%22page%22%3A1%2C%22time%22%3A1571467220592%7D%7D; PHPSESSID=3599fac3910f6745e974205d3744b5ab"
        )?,
    );

    let builder = builder.default_headers(headers);

    let client = builder.build()?;

    // number of pages in search page
    let page_start = str::parse::<i32>(&std::env::args().nth(1).unwrap_or("1".to_string()))
        .expect("Ingresa un número válido.");
    let num_pages = str::parse::<i32>(&std::env::args().nth(2).unwrap_or("97".to_string()))
        .expect("Ingresa un número válido.");

    for i in page_start..num_pages + 1 {
        let client = client.clone();
        match do_search(client, i).await {
            Ok(animes) => {
                use std::fs::OpenOptions;
                use std::io::Write;
                match OpenOptions::new()
                    .create(true)
                    .write(true)
                    .truncate(true)
                    .open(format!("./animes_{}.json", i + 1))
                {
                    Ok(mut file) => {
                        match serde_json::to_string_pretty(&animes) {
                            Ok(json_str) => match write!(file, "{}", json_str) {
                                Err(e) => log::error!("could not write to file: {}", e),
                                _ => {}
                            },
                            Err(e) => {
                                log::error!("could not serialize animes: {}", e);
                            }
                        }
                    }
                    Err(e) => log::error!("could not create file animes_{}.json: {}", i, e),
                }
            }
            Err(e) => {
                log::error!("could not do search: {}", e);
            }
        }
    }

    Ok(())
}

async fn do_search(client: Client, page_id: i32) -> Result<Vec<Anime>, failure::Error> {
    log::info!("Robando pagina: {}", page_id);
    let url = format!("https://monoschinos.com/animes?page={}", page_id);
    let search_res_text = client.get(&url).send().await?.text().await?;

    let urls = {
        let dom = Html::parse_document(&search_res_text);

        let anime_sel_str = "article a.link-anime";
        let anime_sel = Selector::parse("article a.link-anime").map_err(|e| {
            format_err!(
                "could not parse selector {} for page number {}: {:#?}",
                anime_sel_str,
                page_id,
                e
            )
        })?;

        let elems: Vec<_> = dom.select(&anime_sel).collect();
        let mut urls: Vec<String> = Vec::with_capacity(30);
        for anime_el in elems {
            if let Some(anime_url) = anime_el.value().attr("href") {
                urls.push(anime_url.to_owned());
            } else {
                log::error!("could not get link for anime: {:#?}", anime_el.value());
            }
        }
        urls
    };

    let mut animes: Vec<Anime> = Vec::with_capacity(30);
    for url in urls {
        let anime = get_anime(client.clone(), url.clone()).await;
        match anime {
            Ok(anime) => animes.push(anime),
            Err(e) => log::error!("could not get anime in anime link {}: {}", url, e),
        }
    }
    Ok(animes)
}

async fn get_anime(client: Client, anime_url: String) -> Result<Anime, failure::Error> {
    log::info!("Robando anime: {}", anime_url);
    let (mut anime, anime_dom) = {
        macro_rules! get_text {
            ($dom:expr, $selector:expr) => {{
                let sel_res = Selector::parse($selector).map_err(|e| {
                    format_err!(
                        "could not parse selector {}: {:#?} for anime: {}",
                        $selector,
                        e,
                        anime_url
                    )
                });
                match sel_res {
                    Ok(sel) => {
                        if let Some(el) = $dom.select(&sel).next() {
                            el.text().next().unwrap_or("-1").to_owned()
                        } else {
                            "-1".to_owned()
                        }
                    }
                    Err(e) => {
                        log::error!("{}", e);
                        "-1".to_owned()
                    }
                }
            }};
        }

        let anime_page_txt = client.get(&anime_url).send().await?.text().await?;
        let anime_dom = Html::parse_document(&anime_page_txt);

        let score = get_text!(&anime_dom, "div.score");
        let score = score.trim();
        let status = get_text!(&anime_dom, "div.Type small");
        let synopsis = get_text!(&anime_dom, "div.Description p");
        let title = get_text!(&anime_dom, "h1.Title");
        let release_n_type = get_text!(&anime_dom, "div.after-title small");

        let genres_sel_str = "div.generos a";
        let genres_sel = Selector::parse(genres_sel_str).map_err(|e| {
            let err = format_err!(
                "could not parse genres selector {} for anime {}: {:#?}",
                genres_sel_str,
                anime_url,
                e
            );
            log::error!("{}", err);
            err
        });
        let mut genres = Vec::with_capacity(5);
        match genres_sel {
            Ok(sel) => {
                for genre_el in anime_dom.select(&sel) {
                    genres.push(
                        genre_el
                            .text()
                            .next()
                            .unwrap_or_else(|| {
                                log::error!("could not get genres for anime: {}", anime_url);
                                "-1"
                            })
                            .to_owned(),
                    )
                }
            }
            Err(e) => log::error!("{}", e),
        }

        let portrait_sel_str = "header figure img";
        let portrait_sel_res = Selector::parse(portrait_sel_str).map_err(|e| {
            format_err!(
                "could not parse selector {} for anime {}: {:#?}",
                portrait_sel_str,
                anime_url,
                e
            )
        });
        let portrait = match portrait_sel_res {
            Ok(sel) => {
                if let Some(img_el) = anime_dom.select(&sel).next() {
                    img_el.value().attr("src").unwrap_or("-1").to_owned()
                } else {
                    log::error!("could not find img element for anime: {}", anime_url);
                    "-1".to_owned()
                }
            }
            Err(e) => {
                log::error!("{}", e);
                "-1".to_owned()
            }
        };

        let mut release_n_type = release_n_type.split('|');
        let release_date = release_n_type
            .next()
            .unwrap_or("2000-01-01")
            .trim()
            .to_owned();
        let type_ = release_n_type.next().unwrap_or("Anime").trim().to_owned();

        let anime = Anime {
            titulo: title,
            sinopsis: synopsis,
            puntuacion: str::parse::<f32>(&score).unwrap_or(0.0),
            fecha_lanzamiento: release_date,
            tipo: type_,
            estado: status,
            generos: genres,
            portada: portrait,
            episodios: vec![],
        };
        (anime, anime_page_txt)
    };

    let episodes = get_episodes(client.clone(), anime_dom).await?;
    anime.episodios = episodes;

    Ok(anime)
}

async fn get_episodes(client: Client, anime_dom: String) -> Result<Vec<Episodio>, failure::Error> {
    let episodes_requests = {
        let anime_dom = Html::parse_document(&anime_dom);
        let episodes_sel = Selector::parse("div.SerieCaps a")
            .map_err(|e| format_err!("could not parse selector: {:#?}", e))?;

        let episodes_el: Vec<_> = anime_dom.select(&episodes_sel).collect();
        let episodes_el_iter = episodes_el.into_iter().rev().enumerate();

        let mut episodes_requests = Vec::with_capacity(12);
        for episode_el in episodes_el_iter {
            let episode_url = episode_el.1.value().attr("href");
            let episode_url = match episode_url {
                Some(url) => url,
                None => continue,
            };

            episodes_requests.push(get_episode(
                client.clone(),
                episode_el.0 as f32 + 1.0,
                episode_url.to_string(),
            ));
        }
        episodes_requests
    };

    let episodes_res = futures_util::future::join_all(episodes_requests).await;
    let mut episodes = Vec::with_capacity(25);
    for episode_res in episodes_res {
        match episode_res {
            Ok(episode) => episodes.push(episode),
            Err(e) => log::error!("there was an error getting episode: {}", e),
        }
    }

    Ok(episodes)

    //unimplemented!()
}

async fn get_episode(
    client: Client,
    episode_num: f32,
    episode_url: String,
) -> Result<Episodio, failure::Error> {
    let resp = client.get(&episode_url).send().await?.text().await?;
    let dom = Html::parse_document(&resp);

    // get servers names
    let servers_names_sel = Selector::parse("ul.TPlayerNv li")
        .map_err(|e| format_err!("error parsing selector: {:#?}", e))?;

    let mut servers_names = vec![];
    for li in dom.select(&servers_names_sel) {
        let server_name = li.value().attr("title").unwrap_or("Servidor").to_owned();
        servers_names.push(server_name);
    }

    // get servers urls
    let servers_sel = Selector::parse("div.TPlayerTb")
        .map_err(|e| format_err!("error parsing selector: {:#?}", e))?;
    let mut servers_urls = vec![];
    for player_el in dom.select(&servers_sel) {
        let inner_html_escaped = player_el.inner_html();
        let inner_html = htmlescape::decode_html(&inner_html_escaped).map_err(|e| {
            format_err!(
                "error unescaping html on position {}: {:#?}",
                e.position,
                e.kind
            )
        })?;

        let iframe_frag = Html::parse_fragment(&inner_html);
        let iframe_sel = Selector::parse("iframe")
            .map_err(|e| format_err!("error parsing selector: {:#?}", e))?;

        let iframe_el = iframe_frag
            .select(&iframe_sel)
            .next()
            .ok_or_else(|| format_err!("could not select iframe: {:#?}", iframe_frag))?;

        let iframe_url_str = iframe_el
            .value()
            .attr("src")
            .ok_or_else(|| format_err!("error getting src from iframe: {:#?}", iframe_el))?;
        let iframe_url = url::Url::parse(iframe_url_str)?;
        let mut iframe_queries = iframe_url.query_pairs();

        let server_url = iframe_queries
            .find(|(k, _)| k == "url")
            .ok_or_else(|| {
                format_err!("could not find url query parameter on url: {}", iframe_url)
            })?
            .1;

        let server_url = percent_encoding::percent_decode_str(&server_url.into_owned())
            .decode_utf8()?
            .into_owned();
        servers_urls.push(server_url);
    }

    let servers: Vec<Servidor> = servers_names
        .into_iter()
        .zip(servers_urls.into_iter())
        .map(|(name, url)| Servidor {
            nombre: name,
            url: url,
        })
        .collect();

    Ok(Episodio {
        numero: episode_num,
        servidores: servers,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::Future;

    fn test_async_fn<T: Future>(fut: T) -> T::Output {
        let rt = tokio::runtime::Runtime::new().expect("could not create tokio runtime");
        let res = rt.block_on(fut);
        rt.shutdown_on_idle();
        res
    }

    #[test]
    fn test_get_episode() {
        test_async_fn(async {
            let client = reqwest::Client::new();
            let episode = get_episode(
                client,
                1.0,
                "https://monoschinos.com/ver/byousoku-5-centimeter-episodio-1".to_string(),
            )
            .await;
            println!("{:#?}", episode);
        });
    }

    #[test]
    fn test_get_episodes() {
        test_async_fn(async {
            let client = reqwest::Client::new();
            let dom = &client
                .get("https://monoschinos.com/anime/11eyes-sub-espanol")
                .send()
                .await
                .expect("could not send request")
                .text()
                .await
                .expect("could not read response");

            let episodes = get_episodes(client.clone(), dom.to_string()).await;
            println!("{:#?}", episodes);
        });
    }

    #[test]
    fn test_do_search() {
        test_async_fn(async move {
            let client = reqwest::Client::new();
            let animes = do_search(client, 1).await.expect("error doing search");
            println!("{:#?}", animes);
        });
    }
}
