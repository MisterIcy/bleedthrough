<?php

$params = ['x' => $_GET['x']];
system('ls -l ' . $params['x']);
