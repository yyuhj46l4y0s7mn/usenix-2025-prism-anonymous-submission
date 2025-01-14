use csv::Writer;
use dashmap::DashMap;
use eyre::Result;
use goose::prelude::*;
use lazy_static::lazy_static;
use rand::Rng;
use std::{
    fs::{self, File},
    io::{prelude::*, BufReader},
    sync::{Arc, RwLock},
    time::Duration,
};

lazy_static! {
    static ref QUERIES: RwLock<Queries> = RwLock::new(Queries::new().unwrap());
}

#[derive(Debug)]
struct Queries {
    inner: DashMap<usize, Vec<Vec<Arc<str>>>>,
    terms: Vec<Arc<str>>,
}

impl Queries {
    fn new() -> Result<Self> {
        let mut terms: Vec<Arc<str>> = Vec::new();
        let file = File::open("terms_random")?;
        let buffer = BufReader::new(file);
        for line in buffer.lines() {
            if let Ok(term) = line {
                terms.push(Arc::from(&*term));
            }
        }

        Ok(Self {
            terms,
            inner: DashMap::new(),
        })
    }

    fn get_query(&self, user: usize) -> Vec<Arc<str>> {
        let mut queries = self.inner.entry(user).or_insert(Vec::new());
        if queries.is_empty() {
            Queries::prefetch_queries(&self.terms, &mut *queries, 100);
        }
        queries.pop().unwrap()
    }

    fn prefetch_queries(terms: &Vec<Arc<str>>, queries: &mut Vec<Vec<Arc<str>>>, num: usize) {
        for _ in 0..num {
            let mut query = Vec::new();
            let random = rand::thread_rng().gen_range(0..100);
            for dist in [0, 24, 48, 70, 84, 89, 93] {
                if random >= dist {
                    let term_index = rand::thread_rng().gen_range(0..terms.len());
                    query.push(terms[term_index].clone());
                }
            }

            if random > 95 {
                let num_terms = rand::thread_rng().gen_range(0..100);
                for _ in 0..num_terms {
                    let term_index = rand::thread_rng().gen_range(0..terms.len());
                    query.push(terms[term_index].clone());
                }
            }

            queries.push(query);
        }
    }
}

async fn loadtest_index(user: &mut GooseUser) -> TransactionResult {
    // let _goose_metrics = user.get("").await?;
    let query = QUERIES.read().unwrap().get_query(user.weighted_users_index);
    let query = query.join("+");
    let query = format!(
        "/solr/cloudsuite_web_search/query?q={}&lang=en&fl=url&df=text&rows=10&q.op=OR",
        query
    );
    user.get(&query).await?;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let metrics = GooseAttack::initialize()?
        .register_scenario(
            scenario!("LoadtestTransactions")
                .register_transaction(transaction!(loadtest_index))
                .set_wait_time(Duration::from_millis(100), Duration::from_millis(100))?,
        )
        .execute()
        .await?;

    let mut wtr = Writer::from_writer(vec![]);
    let start_time: u64 = metrics
        .history
        .first()
        .unwrap()
        .timestamp
        .timestamp_millis()
        .try_into()
        .unwrap();
    for request in metrics.requests_ {
        wtr.write_record(&[
            (request.0 as u64 + start_time).to_string(),
            (request.0 as u64 + start_time + request.1).to_string(),
            request.1.to_string(),
        ])?;
    }
    let data = String::from_utf8(wtr.into_inner()?)?;
    fs::write("request_stats.csv", data)?;

    let mut wtr = Writer::from_writer(vec![]);
    for test_plan in metrics.history.iter() {
        wtr.write_record(&[
            test_plan.timestamp.timestamp_millis().to_string(),
            test_plan.users.to_string(),
        ])?;
    }
    let data = String::from_utf8(wtr.into_inner()?)?;
    fs::write("test_plan.csv", data)?;

    Ok(())
}
