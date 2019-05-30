document.title = "Scissors Console";

var ctx = document.getElementById("diagnosticPlot");
var plotOptions = {
	scales: {
		yAxes: [{
			scaleLabel: {
				display: true,
				labelString: "Force [V]"
			},
			id: 'left',
			position: 'left'
		}, {
			scaleLabel: {
				display: true,
				labelString: "Position [counts]"
			},
			id: 'right',
			position: 'right'
		}],
		xAxes: [{
			scaleLabel: {
				display: true,
				labelString: "Time [s]",
			}
		}]
	},
	legend: {
		display: true
	},
	tooltips: {
		enabled: false
	},
};

var plotData = {
	labels: [],
	datasets: [
		{
			data: [],
			borderColor: "red",
			backgroundColor: "red",
			label: "F1",
			fill: false,
			yAxisID: "left"
		},
		{
			data: [],
			borderColor: "green",
			backgroundColor: "green",
			label: "F2",
			fill: false,
			yAxisID: "left"
		},
		{
			data: [],
			borderColor: "blue",
			backgroundColor: "blue",
			label: "P",
			fill: false,
			yAxisID: "right"
		}
	]
};

var chart = new Chart(ctx, {
	type: "line",
	data: plotData,
	options: plotOptions
});

var LOG_DATA = document.getElementById("statusLog");

function append_to_log(str) {
	LOG_DATA.value += str;
	LOG_DATA.scrollTop = LOG_DATA.scrollHeight;
}

function clear_log() {
	LOG_DATA.value = "";
}

function update_folder_path(folder) {
	document.getElementById("inputFolderPath").value = folder.toString();
}

function get_file_name() {
	return document.getElementById("inputFilename").value;
}

const MAX_PTS = 100;

function append_to_chart(time, force1, force2, pos) {

	chart.data.labels.push(time.toFixed(2));
	chart.data.datasets[0].data.push(force1);
	chart.data.datasets[1].data.push(force2);
	chart.data.datasets[2].data.push(pos);

	if (chart.data.labels.length > MAX_PTS) {
		chart.data.labels.splice(0, 1);
		chart.data.datasets.forEach((dataset) => {
			dataset.data.splice(0, 1);
		});
	}

	chart.update();
}

function clear_chart() {

	chart.data.labels = [];
	chart.data.datasets.forEach((dataset) => {
		dataset.data = [];
	});
	chart.update();
}

document.getElementById("btnChooseDir").onclick = () => {
	tether("choose_dir");
}

document.getElementById("btnStart").onclick = () => {
	let fname = get_file_name();
	tether("start\n" + fname);
}

document.getElementById("btnStop").onclick = () => {
	tether("stop");
}

document.getElementById("btnClearLog").onclick = () => {
	tether("clear_log");
}
