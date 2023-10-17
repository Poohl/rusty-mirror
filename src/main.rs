extern crate reqwest;
extern crate icalendar;
extern crate itertools;
extern crate serde;

use std::fs;
use std::io::{BufRead, Write};
use std::{io::BufReader, net::TcpListener};
use std::collections::{HashMap, HashSet};
use std::vec::Vec;
use chrono::{NaiveDate, Days, Datelike, Weekday, Timelike, TimeZone};
use itertools::Itertools;
use icalendar::{Component, DatePerhapsTime, Event, Calendar};
use clap::Parser;

mod error_handling {

	#[derive(Debug)]
	pub enum AnError {
		R(reqwest::Error),
		G(String),
		IO(std::io::Error),
		T(std::time::SystemTimeError),
		J(serde_json::Error)
	}

	impl From<reqwest::Error> for AnError {
		fn from(value: reqwest::Error) -> Self {
			AnError::R(value)
		}
	}

	impl From<String> for AnError {
		fn from(value: String) -> Self {
			AnError::G(value)
		}
	}

	impl From<std::io::Error> for AnError {
		fn from(value: std::io::Error) -> Self {
			AnError::IO(value)
		}
	}

	impl From<std::time::SystemTimeError> for AnError{
		fn from(value: std::time::SystemTimeError) -> Self {
			AnError::T(value)
		}
	}

	impl From<serde_json::Error> for AnError {
    fn from(value: serde_json::Error) -> Self {
        AnError::J(value)
    }
}
}

use error_handling::AnError;


mod config {
	use std::{collections::HashMap, io::BufReader, fs::File, net::SocketAddr};
	use clap::Parser;
	use serde::Deserialize;

	use crate::error_handling::AnError;

	type ConfStr = String;
	
	#[derive(Deserialize)]
	pub enum ICalSource {
		URL(ConfStr),
		File(ConfStr),
		CachedURL{url: ConfStr, path: ConfStr, refresh_hours: u64},
		CachedURLwithRefreshAuth{url: ConfStr, path: ConfStr, refresh_hours: u64, token_url: ConfStr, token_body: ConfStr}
	}

	#[derive(Deserialize)]
	pub enum StartDay {
		DayOfWeek(u8), // 0 = Monday
		DayOfMonth(u8), // values other than 0 don't work
		Today
	}

	#[derive(Deserialize)]
	pub struct Config {
		pub wrapper_class : ConfStr,
		pub css_path: ConfStr,
		pub weeks: u8,
		pub week_as_row: bool,
		pub header: bool,
		pub first_day: StartDay,
		pub calendars: HashMap<ConfStr, ICalSource>
	}

	#[derive(Parser, Debug)]
	pub struct Args {
		#[arg(short, long)]
		pub output: Option<String>,
		#[arg(short, long, default_value="config.json")]
		pub config: String,
		#[arg(short, long)]
		pub server: Option<SocketAddr>
	}

	pub fn load_config(path: &str) -> Result<Config, AnError> {
		serde_json::from_reader(BufReader::new(File::open(path)?)).map_err(AnError::from)
	}

}

use crate::config::{load_config, Config, StartDay};

mod download {
	use std::fs;

	use serde::Deserialize;

	use crate::{config::ICalSource, error_handling::AnError};

	#[derive(Deserialize)]
	struct TokenResonse {
		access_token: String,
		scope: String,
		expires_in: i32,
		token_type: String
	}

	fn url_needs_refresh(path: &str, hours: u64) -> bool {
		fs::metadata(path).map_err(AnError::from)
			.and_then(|m|m.modified().map_err(AnError::from))
			.and_then(|t|std::time::SystemTime::now().duration_since(t).map_err(AnError::from))
			.map(|d|d.as_secs() > hours * 60 * 60)
			.unwrap_or(true)
	}

	fn write_cache(path: &str, data: &str) {
		if let Err(e) = fs::write(path, &data) {
			eprintln!("Error writing cached calender to {}: {}", path, e);
		}
	}

	impl ICalSource {
		pub fn load(&self) -> Result<String, AnError> {
			match self {
				ICalSource::URL(url) => reqwest::blocking::get(url)?.text().map_err(AnError::from),
				ICalSource::File(path) => fs::read_to_string(path).map_err(AnError::from),
				ICalSource::CachedURL { url, path, refresh_hours } => {
					if url_needs_refresh(path, *refresh_hours) {
						println!("refreshing cached calendar from {}", url);
						let out = reqwest::blocking::get(url)?.text()?;
						write_cache(path, &out);
						Ok(out)
					} else {
						fs::read_to_string(path).map_err(AnError::from)
					}
				},
				ICalSource::CachedURLwithRefreshAuth { url, path, refresh_hours, token_url, token_body } => {
					if url_needs_refresh(path, *refresh_hours) {
						let client = reqwest::blocking::Client::new();
						println!("refreshing cached calendar from {}, authenticating now...", url);
						let t = client.post(token_url).header("content-type", "application/x-www-form-urlencoded").body(token_body.to_string()).send()?;
						let x = t.json::<TokenResonse>()?;
						let out = client.get(url).bearer_auth(x.access_token).send()?.text()?;
						write_cache(path, &out);
						Ok(out)
					} else {
						fs::read_to_string(path).map_err(AnError::from)
					}
				}
			}
		}
	}

}

fn main() -> Result<(), AnError> {
	println!("Started...");

	let args = config::Args::parse();
	let config : Config = load_config(&args.config)?;

	let mut calendar_day = chrono::offset::Local::now().date_naive();
	let mut calendar = build_calendar(&config, &calendar_day);

	if let Some(out_path) = args.output {
		if let Err(e) = fs::write(&out_path, &calendar) {
			eprintln!("Cannot write calendar to file: {}", e);
		}
	}

	if let Some(addr) = args.server {
		let listener = TcpListener::bind(addr).unwrap();
		for stream in listener.incoming() {
			let today = chrono::offset::Local::now().date_naive();
			if today != calendar_day {
				calendar = build_calendar(&config, &today);
				calendar_day = today;
			}
			let mut stream = stream.unwrap();
			let request: Vec<_> = BufReader::new(&mut stream)
				.lines()
				.map(|l| l.unwrap())
				.take_while(|l| !l.is_empty())
				.collect();
			let head ="HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8";
	
			let css = fs::read_to_string(&config.css_path).unwrap();
			let content = format!("<!DOCTYPE html><html><head><style>{}</style></head><body>{}</body>", css, &calendar);
			let len = content.len();
			let response = format!("{}\r\nContent-Length: {}\r\n\r\n{}", head, len, content);
			stream.write_all(response.as_bytes()).unwrap();
		}
	} 
	Ok(())
}

fn icaltime_to_naive(src: &icalendar::DatePerhapsTime) -> (chrono::NaiveDate, Option<chrono::NaiveTime>) {
	match src {
		DatePerhapsTime::DateTime(cdt) => match cdt {
			icalendar::CalendarDateTime::Floating(ndt) => (ndt.date(),Some(ndt.time())),
			icalendar::CalendarDateTime::Utc(dtu) => {
				let dtl = chrono::Local.from_utc_datetime(&dtu.naive_utc()).naive_local();
				(dtl.date(),Some(dtl.time()))
			},
			icalendar::CalendarDateTime::WithTimezone { date_time, .. } => (date_time.date(), Some(date_time.time())),
		},
		DatePerhapsTime::Date(nd) => (nd.clone(), None)
	}
}

fn simplify_ical_calendar(calendar: Calendar) -> HashMap<NaiveDate, Vec<icalendar::Event>> {
	let mut table: HashMap<NaiveDate, Vec<icalendar::Event>> = HashMap::new();

	calendar.components.iter()
		.filter_map(|c| c.as_event())
		.filter_map(|e| e.get_start().map(|d|(e, icaltime_to_naive(&d).0)))
		//.filter(|(_, d)| start <= d && d <= end)
		.for_each(|(e, d)| table.entry(d).or_insert_with(|| vec![]).push(e.clone()));

	table
}

fn build_calendar(config: &Config, today: &NaiveDate) -> String {
	println!("Starting to build calendar...");
	let now_dow = today.weekday();
	let start_at = today.checked_sub_days(Days::new(match config.first_day {
		StartDay::DayOfWeek(w) => u64::from(
			if now_dow.num_days_from_monday() < u32::from(w) {
				7 + now_dow.num_days_from_monday() - u32::from(w)
			} else {
				now_dow.num_days_from_monday() - u32::from(w)
			}),
		StartDay::DayOfMonth(m) => today.day0().into(),
		StartDay::Today => 0,
	})).unwrap();
	let end_at = start_at.checked_add_days(Days::new((7 * config.weeks).into())).unwrap();

	println!("at {}, calendar will start at {} and run until {}", today, start_at, end_at);
	
	let dense = config.calendars.iter().filter_map(|(name, source)|{
		source.load()
			.map(|s|icalendar::parser::unfold(&s))
			.and_then(|s|Ok(icalendar::Calendar::from(icalendar::parser::read_calendar(&s)?)))
			.map(simplify_ical_calendar)
			.map(|c1| {
				println!("got calender {} with {} events", name, c1.len());

				start_at.iter_days()
					.take_while(|i|i < &end_at)
					.map(|d|(d, c1.get(&d)))
					.map(
					|(d, ove)|
					(d, ove.map(
						|ve|
						ve.into_iter().map(
							|e|
							to_display(e, &name)
						).collect_vec())
					)
				).collect_vec()
			}).map_err(|e| {
				eprintln!("Error getting calendar {}: {:?}", name, e);
				e
			}).ok()
	}).reduce(|old, new|{
		old.into_iter().zip(new.into_iter()).map(
			|((d_o, ove_o), (d_n, ove_n))|
			(d_o, IntoIterator::into_iter([ove_o, ove_n]).filter_map(|x|x).reduce(|l, r|IntoIterator::into_iter([l, r]).flat_map(|ve|ve.into_iter()).collect_vec()))
		).collect_vec()
	});

	println!("Creating table...");
	let table = create_html_calendar(dense.unwrap(), config.week_as_row, config.header, &today, Some(&config.wrapper_class)).unwrap();
	table
}

struct DisplayEvent<'a> {
	title: String,
	time: Option<chrono::NaiveTime>,
	classes: Vec<&'a str>,
}

fn to_display<'a>(e: &Event, calender_name: &'a str) -> DisplayEvent<'a> {
	DisplayEvent {
		title: e.get_summary().unwrap_or("<no title>").to_string(),
		time: e.get_start().and_then(|s|icaltime_to_naive(&s).1),
		classes: if e.get_sequence().is_some(){ vec![calender_name, "recurring"]} else {vec![calender_name]},
	}
}

fn wrap_and_join(what: &mut dyn Iterator<Item = &str>, lhs: &str, join: &str, rhs: &str, alt: Option<&str>) -> String {
	let o = Itertools::join(what, join);
	if o.len() > 0 {
		format!("{}{}{}", lhs, o, rhs)
	} else {
		alt.unwrap_or("").to_string()
	}
}

fn make_html_element(tag: &str, attributes: Option<&str>, content: &str) -> String {
	if let Some(att) = attributes {
		format!("<{} {}>{}</{}>", tag, att, content, tag)
	} else {
		format!("<{}>{}</{}>", tag, content, tag)
	}
}

fn make_html_element2(tag: &str, attributes: &HashMap<&str, &str>, content: &str) -> String {
	format!("<{} {}>{}</{}>", tag, attributes.into_iter().map(|(k, v)| vec![k, "=\"", v, "\""].join("")).join(" "), content, tag)
}

fn create_html_day(d: &NaiveDate, events: &Vec<DisplayEvent>, today: &NaiveDate) -> String {
	let mut day_classes: HashSet<&str> = HashSet::new();
	let d_str = d.weekday().to_string();
	day_classes.insert(&d_str);
	if d < today {
		day_classes.insert("past");
	} else if d == today {
		day_classes.insert("today");
	} else if d > today {
		day_classes.insert("future");
	} else {
		panic!("{} was in no relation to {}", d, today);
	}

	events.iter()
		.flat_map(|e| e.classes.iter())
		.for_each(|c| {day_classes.insert(c);});

	let attrs = wrap_and_join(&mut day_classes.into_iter(), "class=\"", " ", "\"", None);

	let mut content = make_html_element("div", None, &(d.day0() + 1).to_string());

	content.push_str(&events.iter().map(|e|create_html_event(e)).join(""));

	make_html_element("td", Some(&attrs), &content)
}

fn create_html_event(e: &DisplayEvent) -> String {
	let attributes = wrap_and_join(&mut e.classes.iter().map(|s|*s), "class=\"", " ", "\"", None);

	let content = if let Some(t) = e.time {
		std::borrow::Cow::Owned(format!("{: >2}:{:0>2} {}", t.hour(), t.minute(), e.title))
	} else {
		std::borrow::Cow::Borrowed(&e.title)
	};
	make_html_element("div", Some(&attributes), &content)
}

fn create_html_calendar(calendar: Vec<(NaiveDate, Option<Vec<DisplayEvent>>)>, row_weeks: bool, header: bool, today: &NaiveDate, wrapper_class: Option<&str>) -> Option<String> {
	let next_column = if row_weeks {1} else {7};
	let next_row = if row_weeks {7} else {1};
	let mut content = if let Some(class) = wrapper_class {
		format!("<table class=\"{}\">", class)
	} else {
		"<table>".to_string()
	};
	let empty_vec = vec![];
	
	fn make_header_cell(day: &Weekday) -> String {
		make_html_element("th", Some(&format!("class:\"{}\"", day)), 
		&make_html_element("div", Some(&format!("class=\"{}\"", day)), &day.to_string()))
	}

	if row_weeks && header {
		content.push_str("<tr>");
		let mut current_day = calendar[0].0.weekday();
		for _ in 0..7 {
			content.push_str(&make_header_cell(&current_day));
			current_day = current_day.succ();
		}
		content.push_str("</tr>");
	}

	let mut current = 0;

	while current < calendar.len() {
		content.push_str("<tr>");
		if !row_weeks && header {
			content.push_str(&make_header_cell(&calendar[current].0.weekday()));
		}

		for _ in 0..7 {
			content.push_str(&create_html_day(&calendar[current].0, &calendar[current].1.as_ref().unwrap_or(&empty_vec), today));
			current += next_column;
		}
		current = current + next_row - 7*next_column;
		content.push_str("</tr>");
	}
	content.push_str("</table>");
	Some(content)
}
