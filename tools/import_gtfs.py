"""Turn selected PID GTFS route patterns into a local simulator routes.tsv."""
import argparse
from collections import defaultdict
import csv
import io
from pathlib import Path
import re
from zipfile import ZipFile

MODES = {"0": "tram", "3": "bus", "11": "trolley"}


def rows(archive, filename):
    with archive.open(filename + ".txt") as stream:
        yield from csv.DictReader(io.TextIOWrapper(stream, encoding="utf-8-sig"))


def seconds(value):
    hours, minutes, seconds_ = map(int, value.split(":"))
    return 3600 * hours + 60 * minutes + seconds_


def transfers(icons):
    marks = [m.upper() for m in re.findall(r"M([abc])", icons, flags=re.IGNORECASE)]
    if re.search(r"(?:R[a-z]|S[a-z])", icons):
        marks.append("S")
    return "".join(dict.fromkeys(marks))


def write_patterns(archive, wanted, variants, prague=False):
    lines = {
        row["route_id"]: (row["route_short_name"], MODES[row["route_type"]])
        for row in rows(archive, "routes")
        if row["route_type"] in MODES and (
            (prague and row.get("is_regional") == "0") or row["route_short_name"] in wanted
        )
    }
    missing = wanted - {line for line, _ in lines.values()}
    if missing:
        raise ValueError("В архиве нет нужных автобусных, троллейбусных или трамвайных линий: " + ", ".join(sorted(missing)))
    trips = {
        row["trip_id"]: (row["route_id"], row["direction_id"])
        for row in rows(archive, "trips") if row["route_id"] in lines
    }
    stops = {row["stop_id"]: (row["stop_name"], row["zone_id"]) for row in rows(archive, "stops")}
    prague_stops = {row["stop_id"] for row in rows(archive, "stops") if row.get("zone_region_type") == "1"}
    times = defaultdict(list)
    for row in rows(archive, "stop_times"):
        trip = row["trip_id"]
        if trip not in trips or row["stop_id"] not in stops:
            continue
        if row.get("trip_operation_type") not in (None, "", "1") or (row["pickup_type"] == row["drop_off_type"] == "1"):
            continue
        value = row["arrival_time"] or row["departure_time"]
        if value:
            times[trip].append((int(row["stop_sequence"]), row["stop_id"], seconds(value), transfers(row.get("stop_icons", ""))))

    groups = defaultdict(dict)
    for trip, points in times.items():
        points.sort()
        if len(points) < 2 or any(a[2] > b[2] for a, b in zip(points, points[1:])):
            continue
        route_id, _ = trips[trip]
        if prague and lines[route_id][0] not in wanted and not any(p[1] in prague_stops for p in points):
            continue
        group = groups[trips[trip]]
        pattern = tuple(stops[point[1]][0] for point in points)
        count, sample = group.get(pattern, (0, points))
        group[pattern] = (count + 1, sample)

    result = [
        "# Source: Pražská integrovaná doprava / ROPID, https://pid.cz/en/opendata/",
        "# License: CC BY 4.0, https://creativecommons.org/licenses/by/4.0/",
        "# Modified: selected transit patterns and relative timetable seconds for simulation.",
    ]
    found = set()
    def order(entry):
        (route_id, direction), _ = entry
        line, mode = lines[route_id]
        return (mode, not line.isdecimal(), int(line) if line.isdecimal() else line, direction, route_id)

    for (route_id, direction), group in sorted(groups.items(), key=order):
        line, mode = lines[route_id]
        ranked = sorted(group.items(), key=lambda entry: (entry[1][0], len(entry[0])), reverse=True)
        distinct, similar = [], []
        endpoints = set()
        for names, (_, points) in ranked:
            termini = (names[0], names[-1])
            target = similar if termini in endpoints else distinct
            target.append(points)
            endpoints.add(termini)
        for points in (distinct + similar)[:variants]:
            result.append(f"route\t{line}\t{mode}")
            found.add(line)
            start = points[0][2]
            for _, stop_id, time, marks in points:
                name, zone = stops[stop_id]
                name = " ".join(name.split())
                result.append(f"stop\t{stop_id}\t{name}\t{zone}\t{marks}\t{time - start}")
            result.append("")
    if wanted - found:
        raise ValueError("Нет пригодных рейсов для: " + ", ".join(sorted(wanted - found)))
    return "\n".join(result)


def main():
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument("archive", type=Path, help="PID_GTFS.zip from PID Open Data")
    cli.add_argument("lines", nargs="*", help="Line numbers, for example 22 59 152")
    cli.add_argument("--prague", action="store_true", help="Include all Prague urban tram, bus and trolleybus lines")
    cli.add_argument("-o", "--output", type=Path, default=Path("routes.tsv"))
    cli.add_argument("--variants", type=int, choices=range(1, 6), default=1,
                     help="Number of frequent stop patterns per direction (1–5)")
    args = cli.parse_args()
    if not args.lines and not args.prague:
        cli.error("provide line numbers or --prague")
    try:
        with ZipFile(args.archive) as archive:
            result = write_patterns(archive, set(args.lines), args.variants, args.prague)
        temp = args.output.with_suffix(args.output.suffix + ".tmp")
        temp.write_text(result, encoding="utf-8")
        temp.replace(args.output)
    except (OSError, KeyError, ValueError) as exc:
        cli.exit(1, f"{exc}\n")
    print(args.output)


if __name__ == "__main__":
    main()
