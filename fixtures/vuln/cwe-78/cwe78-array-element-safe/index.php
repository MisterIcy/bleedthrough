<?php

$params = ['x' => $_GET['x']];
system('ls -l ' . escapeshellarg($params['x']));
