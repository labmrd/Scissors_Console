document.title = "Scissors Console";

var ctx = document.getElementById("diagnosticPlot");
var plotOptions = {
	scales: {
		yAxes: [{
			scaleLabel: {
				display: true,
				labelString: "Force (lbf)"
			}
		}],
		xAxes: [{
			scaleLabel: {
				display: true,
				labelString: "Time (ms)"
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
			label: "F",
			fill: false
		},
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

function append_to_chart(time, force) {
	chart.data.labels.push(time);
	chart.data.datasets[0].data.push(force);
	chart.update();
}

function clear_chart() {
	chart.data.labels = [];
	chart.data.datasets[0].data = [];
}

document.getElementById("btnChooseDir").onclick = () => {
	window.tether("choose_dir");
}

document.getElementById("btnStart").onclick = () => {
	let fname = get_file_name();
	window.tether("start\n" + fname);
}

document.getElementById("btnStop").onclick = () => {
	window.tether("stop");
}

document.getElementById("btnClearLog").onclick = () => {
	window.tether("clear_log");
}
